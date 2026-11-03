/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (Uids, Postings). Changed for ultragraphdb: translated to Rust; the
 * calculated-uid cache Dgraph serves unbounded reads from is not ported (stage-2 design, Non-Goals), so every read
 * takes the paths Dgraph takes for a list without it.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::ops::ControlFlow;

use ultragraphdb_core_algo::{intersect_compressed_with, intersect_with};
use ultragraphdb_core_codec::{List as UidList, Posting, PostingType};
use ultragraphdb_core_ids::SystemId;

use crate::errors::posting_error::PostingError;
use crate::types::list::List;
use crate::types::list_options::ListOptions;
use crate::types::p_iterator::{PIterator, PPosting};

impl List {
    /// The IDs of the list's reference postings at the options' read timestamp: Dgraph's `Uids`. A pack holds
    /// references' IDs alone, a value's key being its fingerprint, which no ID equals.
    ///
    /// # Errors
    ///
    /// [`PostingError::ts_too_old`] for an intersection read below the list's complete version, and the errors of
    /// [`List::iterate`].
    pub fn uids(&self, opt: &ListOptions) -> Result<UidList, PostingError> {
        let requested = opt.first_count();
        let first = if requested == 0 {
            i64::from(i32::MAX)
        } else {
            requested
        };
        let limit = usize::try_from(first).ok().filter(|&n| n > 0);
        let mut out = UidList::default();
        let intersect = opt.intersect_with();

        if let Some(with) = intersect
            && self.mutation.is_empty()
            && self.plist.splits().is_empty()
        {
            if opt.read_ts() < self.min_ts {
                return Err(PostingError::ts_too_old());
            }
            intersect_compressed_with(self.plist.pack(), opt.after_uid(), with, &mut out);
            let postings = self.plist.postings();
            out.ids_mut()
                .retain(|id| reference_in(postings, id.to_u128()));
            return Ok(out);
        }

        let approx = self.approx_len();
        if let Some(with) = intersect
            && with.ids().len() < approx
        {
            // Few IDs to look for: look each one up.
            for &id in with.ids() {
                if self.has_reference(opt.read_ts(), id.to_u128())? {
                    out.ids_mut().push(id);
                }
            }
            return Ok(out);
        }

        // Iterating, only the range of the intersection is read.
        let (uid_min, uid_max) = intersect
            .and_then(|w| Some((w.ids().first()?.to_u128(), w.ids().last()?.to_u128())))
            .unwrap_or((0, 0));
        self.iterate(opt.read_ts(), opt.after_uid(), |p| {
            if p.posting_type() == PostingType::Ref {
                if p.uid() < uid_min {
                    return Ok::<_, PostingError>(ControlFlow::Continue(()));
                }
                if p.uid() > uid_max && uid_max > 0 {
                    return Ok(ControlFlow::Break(()));
                }
                let id = SystemId::from_u128(p.uid()).map_err(|_| {
                    PostingError::corrupted(&self.key, "a reference keyed by no ID")
                })?;
                out.ids_mut().push(id);
                if limit.is_some_and(|n| out.ids().len() > n) {
                    return Ok(ControlFlow::Break(()));
                }
            }
            Ok(ControlFlow::Continue(()))
        })?;

        if let Some(with) = intersect {
            let mut intersected = UidList::default();
            intersect_with(&out, with, &mut intersected);
            out = intersected;
        }
        // Only a positive count is applied here: a negative one is the query layer's, over the filtered result.
        if let Some(n) = limit {
            out.ids_mut().truncate(n);
        }
        Ok(out)
    }

    /// Whether a reference posting of `uid` is held at `read_ts`, from the deltas or the immutable layer, where a uid
    /// the pack holds without a posting beside it is a reference's.
    fn has_reference(&self, read_ts: u64, uid: u128) -> Result<bool, PostingError> {
        let (search_further, found) =
            self.mutation()
                .find_posting_where(read_ts, uid, is_reference);
        if found.is_some() || !search_further {
            return Ok(found.is_some());
        }
        // Iteration starts after its argument, so it seeks the uid before.
        let mut pitr = PIterator::seek(self, uid.saturating_sub(1), 0)?;
        // Ends: each turn moves the iterator to its next posting.
        while pitr.valid() {
            match pitr.posting() {
                PPosting::Uid(u) if u == uid => return Ok(true),
                PPosting::Found(p) if p.uid() == uid && is_reference(p) => return Ok(true),
                PPosting::Found(p) if p.uid() == uid => pitr.next(),
                PPosting::Found(_) | PPosting::Uid(_) => return Ok(false),
            }
        }
        Ok(false)
    }

    /// Calls `f` with each reference posting at the options' read timestamp above its after-uid: Dgraph's
    /// `Postings`.
    pub fn postings<E, F>(&self, opt: &ListOptions, mut f: F) -> Result<(), E>
    where
        E: From<PostingError>,
        F: FnMut(&Posting) -> Result<ControlFlow<()>, E>,
    {
        self.iterate(opt.read_ts(), opt.after_uid(), |p| {
            if p.posting_type() != PostingType::Ref {
                return Ok(ControlFlow::Continue(()));
            }
            f(p)
        })
    }
}

fn is_reference(p: &Posting) -> bool {
    p.posting_type() == PostingType::Ref
}

/// Whether `uid` of a complete list's pack is a reference's: no posting of it is kept beside the pack, or one of those
/// kept is a reference.
fn reference_in(postings: &[Posting], uid: u128) -> bool {
    let from = postings.partition_point(|p| p.uid() < uid);
    let mut held = postings[from..]
        .iter()
        .take_while(|p| p.uid() == uid)
        .peekable();
    held.peek().is_none() || held.any(is_reference)
}
