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
use ultragraphdb_proto_pb::pb::{self, Posting, posting};

use crate::errors::posting_error::PostingError;
use crate::types::list::List;
use crate::types::list_options::ListOptions;

impl List {
    /// The uids of the list's reference postings at the options' read timestamp: Dgraph's `Uids`.
    ///
    /// # Errors
    ///
    /// [`PostingError::TsTooOld`] for an intersection read below the list's complete version, and the errors of
    /// [`List::iterate`].
    pub fn uids(&self, opt: &ListOptions) -> Result<pb::List, PostingError> {
        let requested = opt.first_count();
        let first = if requested == 0 {
            i64::from(i32::MAX)
        } else {
            requested
        };
        let mut out = pb::List::default();
        let intersect = opt.intersect_with();

        if let Some(with) = intersect
            && self.mutation.is_empty()
            && self.plist.splits.is_empty()
        {
            if opt.read_ts() < self.min_ts {
                return Err(PostingError::TsTooOld());
            }
            intersect_compressed_with(self.plist.pack.as_ref(), opt.after_uid(), with, &mut out);
            return Ok(out);
        }

        let approx = self.approx_len();
        if let Some(with) = intersect
            && with.uids.len() < approx
        {
            // Few uids to look for: look each one up.
            for &uid in &with.uids {
                if self.find_posting(opt.read_ts(), uid)?.is_some() {
                    out.uids.push(uid);
                }
            }
            return Ok(out);
        }

        // Iterating, only the range of the intersection is read.
        let (uid_min, uid_max) = intersect
            .and_then(|w| Some((*w.uids.first()?, *w.uids.last()?)))
            .unwrap_or((0, 0));
        self.iterate(opt.read_ts(), opt.after_uid(), |p| {
            if p.posting_type == posting::PostingType::Ref as i32 {
                if p.uid < uid_min {
                    return Ok::<_, PostingError>(ControlFlow::Continue(()));
                }
                if p.uid > uid_max && uid_max > 0 {
                    return Ok(ControlFlow::Break(()));
                }
                out.uids.push(p.uid);
                if first > 0 && out.uids.len() as i64 > first {
                    return Ok(ControlFlow::Break(()));
                }
            }
            Ok(ControlFlow::Continue(()))
        })?;

        if let Some(with) = intersect {
            let mut intersected = pb::List::default();
            intersect_with(&out, with, &mut intersected);
            out = intersected;
        }
        // Only a positive count is applied here: a negative one is the query layer's, over the filtered result.
        if first > 0 && out.uids.len() as i64 > first {
            out.uids.truncate(first as usize);
        }
        Ok(out)
    }

    /// Calls `f` with each reference posting at the options' read timestamp above its after-uid: Dgraph's
    /// `Postings`.
    pub fn postings<E, F>(&self, opt: &ListOptions, mut f: F) -> Result<(), E>
    where
        E: From<PostingError>,
        F: FnMut(&Posting) -> Result<ControlFlow<()>, E>,
    {
        self.iterate(opt.read_ts(), opt.after_uid(), |p| {
            if p.posting_type != posting::PostingType::Ref as i32 {
                return Ok(ControlFlow::Continue(()));
            }
            f(p)
        })
    }
}
