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

use crate::errors::posting_error::PostingError;
use crate::types::list::List;
use crate::types::list_options::ListOptions;
use crate::types::p_iterator::{PIterator, PPosting};

impl List {
    /// The uids of the list's reference postings at the options' read timestamp: Dgraph's `Uids`. A quad list's pack
    /// holds its literals' fingerprints as uids, so the paths that read the pack or look a uid up leave out a uid
    /// held by value postings alone, as iterating does.
    ///
    /// # Errors
    ///
    /// [`PostingError::TsTooOld`] for an intersection read below the list's complete version, and the errors of
    /// [`List::iterate`].
    pub fn uids(&self, opt: &ListOptions) -> Result<UidList, PostingError> {
        let requested = opt.first_count();
        let first = if requested == 0 {
            i64::from(i32::MAX)
        } else {
            requested
        };
        let mut out = UidList::default();
        let intersect = opt.intersect_with();

        if let Some(with) = intersect
            && self.mutation.is_empty()
            && self.plist.splits().is_empty()
        {
            if opt.read_ts() < self.min_ts {
                return Err(PostingError::TsTooOld());
            }
            intersect_compressed_with(self.plist.pack(), opt.after_uid(), with, &mut out);
            let postings = self.plist.postings();
            out.uids_mut().retain(|&uid| reference_in(postings, uid));
            return Ok(out);
        }

        let approx = self.approx_len();
        if let Some(with) = intersect
            && with.uids().len() < approx
        {
            // Few uids to look for: look each one up.
            for &uid in with.uids() {
                if self.has_reference(opt.read_ts(), uid)? {
                    out.uids_mut().push(uid);
                }
            }
            return Ok(out);
        }

        // Iterating, only the range of the intersection is read.
        let (uid_min, uid_max) = intersect
            .and_then(|w| Some((*w.uids().first()?, *w.uids().last()?)))
            .unwrap_or((0, 0));
        self.iterate(opt.read_ts(), opt.after_uid(), |p| {
            if p.posting_type() == PostingType::Ref {
                if p.uid() < uid_min {
                    return Ok::<_, PostingError>(ControlFlow::Continue(()));
                }
                if p.uid() > uid_max && uid_max > 0 {
                    return Ok(ControlFlow::Break(()));
                }
                out.uids_mut().push(p.uid());
                if first > 0 && out.uids().len() as i64 > first {
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
        if first > 0 && out.uids().len() as i64 > first {
            out.uids_mut().truncate(first as usize);
        }
        Ok(out)
    }

    /// Whether a reference posting of `uid` is held at `read_ts`, from the deltas or the immutable layer, where a uid
    /// the pack holds without a posting beside it is a reference's.
    fn has_reference(&self, read_ts: u64, uid: u64) -> Result<bool, PostingError> {
        let (search_further, found) =
            self.mutation()
                .find_posting_where(read_ts, uid, is_reference);
        if found.is_some() || !search_further {
            return Ok(found.is_some());
        }
        // Iteration starts after its argument, so it seeks the uid before.
        let mut pitr = PIterator::seek(self, uid.wrapping_sub(1), 0)?;
        while pitr.valid() {
            match pitr.posting() {
                PPosting::Uid(u) if u == uid => return Ok(true),
                PPosting::Found(p) if p.uid() == uid && is_reference(p) => return Ok(true),
                PPosting::Found(p) if p.uid() == uid => pitr.next(),
                _ => return Ok(false),
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
fn reference_in(postings: &[Posting], uid: u64) -> bool {
    let from = postings.partition_point(|p| p.uid() < uid);
    let mut held = postings[from..]
        .iter()
        .take_while(|p| p.uid() == uid)
        .peekable();
    held.peek().is_none() || held.any(is_reference)
}
