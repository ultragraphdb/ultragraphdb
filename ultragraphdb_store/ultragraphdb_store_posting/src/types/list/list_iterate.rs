/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (pickPostings, iterate, Iterate, IsEmpty, Length, length).
 * Changed for ultragraphdb: translated to Rust; the callback says whether to go on with `ControlFlow` where Dgraph
 * returns ErrStopIteration; postings of one uid are ordered by their layer, the request's own newest, where Dgraph
 * orders them by an effective timestamp that ties between a commit at the read timestamp and the request's own
 * write; Dgraph's `log.Fatalf` on an unhandled case of the merge has no counterpart, the merge's cases being
 * exhaustive.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::ops::ControlFlow;

use ultragraphdb_proto_pb::pb::{Posting, posting};

use crate::errors::posting_error::PostingError;
use crate::types::list::List;
use crate::types::mutable_layer::Rank;
use crate::types::p_iterator::{PIterator, PPosting};
use crate::utils::constants::DEL;

impl List {
    /// The timestamp of the newest delete-all visible at `read_ts`, 0 for none, and the postings of the deltas from
    /// it on, sorted by uid and, for one uid, newest first: Dgraph's `pickPostings`.
    pub fn pick_postings(&self, read_ts: u64) -> (u64, Vec<&Posting>) {
        let (marker, layers) = self.mutation.live(read_ts);
        let delete_below_ts = match marker {
            Some(Rank::Committed(ts)) => ts,
            Some(_) => self.mutation.read_ts().max(1),
            None => 0,
        };
        let mut posts: Vec<(Rank, usize, &Posting)> = layers
            .iter()
            .flat_map(|(rank, pl)| pl.postings.iter().enumerate().map(|(i, p)| (*rank, i, p)))
            .collect();
        posts.sort_by(|(ra, ia, pa), (rb, ib, pb)| {
            pa.uid.cmp(&pb.uid).then(rb.cmp(ra)).then(ib.cmp(ia))
        });
        (
            delete_below_ts,
            posts.into_iter().map(|(_, _, p)| p).collect(),
        )
    }

    /// Calls `f` with each posting of the list at `read_ts` whose uid is above `after_uid`, in uid order, the
    /// deltas merged over the immutable layer, until `f` breaks: Dgraph's `Iterate`.
    ///
    /// # Errors
    ///
    /// [`PostingError::ReadBelowMinTs`] when `read_ts` is below the list's complete version, a missing part, or
    /// the error `f` returns.
    pub fn iterate<E, F>(&self, read_ts: u64, after_uid: u64, mut f: F) -> Result<(), E>
    where
        E: From<PostingError>,
        F: FnMut(&Posting) -> Result<ControlFlow<()>, E>,
    {
        let (delete_below_ts, mposts) = self.pick_postings(read_ts);
        if read_ts < self.min_ts {
            return Err(
                PostingError::ReadBelowMinTs(read_ts, self.min_ts, self.key.clone()).into(),
            );
        }
        let mut midx = if after_uid > 0 {
            mposts.partition_point(|mp| mp.uid <= after_uid)
        } else {
            0
        };
        let mut pitr = PIterator::seek(self, after_uid, delete_below_ts)?;
        let mut prev_uid = 0u64;
        let (mut read, mut deleted) = (0usize, 0usize);
        let mut count = |mp: Option<&Posting>| match mp {
            Some(p) if p.op == DEL => deleted += 1,
            _ => read += 1,
        };
        let result = loop {
            let mp = mposts.get(midx).copied();
            let mp_uid = mp.map_or(0, |p| p.uid);
            let pp = if pitr.valid() {
                Some(pitr.posting())
            } else {
                None
            };
            let pp_uid = pp.map_or(0, |p| p.uid());
            let flow = if mp_uid > 0 && mp_uid == prev_uid {
                // Only the newest version of a uid's posting counts.
                midx += 1;
                Ok(ControlFlow::Continue(()))
            } else if pp_uid == 0 && mp_uid == 0 {
                break Ok(());
            } else if mp_uid == 0 || (pp_uid > 0 && pp_uid < mp_uid) {
                count(None);
                let flow = call(&mut f, pp);
                pitr.next();
                flow
            } else if pp_uid == 0 || (mp_uid > 0 && mp_uid < pp_uid) {
                count(mp);
                prev_uid = mp_uid;
                midx += 1;
                emit(&mut f, mp)
            } else {
                // Both iterators are at the same uid: the delta's posting replaces the immutable one.
                count(mp);
                prev_uid = mp_uid;
                pitr.next();
                midx += 1;
                emit(&mut f, mp)
            };
            match flow {
                Ok(ControlFlow::Continue(())) => {}
                Ok(ControlFlow::Break(())) => break Ok(()),
                Err(error) => break Err(error),
            }
        };
        // Many deleted postings mean rollups lag behind the deletes.
        if read < deleted {
            tracing::trace!(
                key = ?self.key,
                total = read + deleted,
                percent_deleted = deleted * 100 / (read + deleted),
                "high proportion of deleted data observed for posting list"
            );
        }
        result
    }

    /// Whether no posting lies above `after_uid` at `read_ts`: Dgraph's `IsEmpty`.
    pub fn is_empty(&self, read_ts: u64, after_uid: u64) -> Result<bool, PostingError> {
        let mut count = 0usize;
        self.iterate(read_ts, after_uid, |_| {
            count += 1;
            Ok::<_, PostingError>(ControlFlow::Break(()))
        })?;
        Ok(count == 0)
    }

    /// The number of postings above `after_uid` at `read_ts`, counted by iterating: Dgraph's `Length`, which
    /// returns -1 where the iteration fails.
    pub fn length(&self, read_ts: u64, after_uid: u64) -> i64 {
        let mut count = 0i64;
        let result = self.iterate(read_ts, after_uid, |_| {
            count += 1;
            Ok::<_, PostingError>(ControlFlow::Continue(()))
        });
        if result.is_err() { -1 } else { count }
    }
}

/// Calls `f` with the immutable layer's posting, or with a uid posting for a uid it holds no posting for.
fn call<E, F>(f: &mut F, pp: Option<PPosting<'_>>) -> Result<ControlFlow<()>, E>
where
    F: FnMut(&Posting) -> Result<ControlFlow<()>, E>,
{
    match pp {
        Some(PPosting::Found(p)) => f(p),
        Some(PPosting::Uid(uid)) => f(&Posting {
            uid,
            val_type: posting::ValType::Uid as i32,
            ..Posting::default()
        }),
        None => Ok(ControlFlow::Continue(())),
    }
}

/// Calls `f` with a delta's posting unless it is a delete.
fn emit<E, F>(f: &mut F, mp: Option<&Posting>) -> Result<ControlFlow<()>, E>
where
    F: FnMut(&Posting) -> Result<ControlFlow<()>, E>,
{
    match mp {
        Some(p) if p.op != DEL => f(p),
        _ => Ok(ControlFlow::Continue(())),
    }
}
