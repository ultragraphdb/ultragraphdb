/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (Rollup, rollup, encode). Changed for ultragraphdb: translated to
 * Rust; the rollup's writes are store entries, its parts at the main key's version and the parts it removes
 * deleted (see `RollupOutput`); the split threshold is a parameter; a uid past the end of the next part as well
 * moves the encoding on to the part holding it, where Dgraph's moves one part per uid and puts that uid in a part
 * whose range does not hold it, which a read starting in its own part then misses; a bare reference whose uid a
 * posting kept beside the pack shares, a literal of a quad list whose fingerprint equals it, is kept whole too, where
 * Dgraph's lists hold one kind of posting and need not tell the two apart.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::ops::ControlFlow;

use ultragraphdb_core_codec::{Encoder, PostingList, PostingType};
use ultragraphdb_traits_store::Entry;

use crate::errors::posting_error::PostingError;
use crate::types::list::List;
use crate::types::rollup_output::RollupOutput;
use crate::utils::constants::BLOCK_SIZE;

impl List {
    /// The writes that roll the list up at `read_ts`, and the version they are written at: Dgraph's `Rollup`. The
    /// version is the newest commit the rollup read, plus one when `read_ts` is finite, so that no commit is
    /// overwritten; the main key, every part and the deletion of every part removed are written at it. A list or
    /// part of at least `max_list_size` bytes over more than one block is split. `None` when `read_ts` is below the
    /// list's complete version.
    pub fn rollup_entries(
        &self,
        read_ts: u64,
        max_list_size: usize,
    ) -> Result<Option<(u64, Vec<Entry>)>, PostingError> {
        let Some(out) = self.rollup(read_ts, true, max_list_size)? else {
            return Ok(None);
        };
        let mut version = out.new_min_ts();
        if read_ts != u64::MAX {
            version += 1;
        }
        let entries = out.entries(&self.key, self.plist.splits(), version)?;
        Ok(Some((version, entries)))
    }

    /// Merges the deltas at or below `read_ts` into the immutable layer: Dgraph's `rollup`. With `split`, a list
    /// too large is split and empty parts are removed; without it, the output is one list.
    pub fn rollup(
        &self,
        read_ts: u64,
        split: bool,
        max_list_size: usize,
    ) -> Result<Option<RollupOutput>, PostingError> {
        if self.min_ts > read_ts {
            return Ok(None);
        }
        let mut out = RollupOutput::new(
            PostingList::default().with_splits(self.plist.splits().to_vec()),
            0,
        );
        if !self.plist.splits().is_empty() || !self.mutation.is_empty() {
            self.encode(&mut out, read_ts, split)?;
        } else {
            out = RollupOutput::new((*self.plist).clone(), 0);
        }

        // Iterating skips postings deleted to give a sorted view, so the newest commit is taken from every posting
        // picked at read_ts, the delete-all included.
        let (delete_below_ts, mposts) = self.pick_postings(read_ts);
        let max_commit_ts = mposts
            .iter()
            .map(|p| p.commit_ts())
            .fold(self.min_ts.max(delete_below_ts), u64::max);
        out.set_new_min_ts(max_commit_ts);
        if split {
            out.recursive_split(max_list_size);
            out.remove_empty_splits();
        } else {
            out.plist_mut().set_splits(Vec::new());
        }
        Ok(Some(out))
    }

    /// Packs the list's postings at `read_ts` into the output: into its parts when the list is split and `split`
    /// is set, into one list otherwise. Value postings, postings with facets or statement IDs, and a bare reference
    /// sharing its uid with one of those are kept beside the uids.
    fn encode(
        &self,
        out: &mut RollupOutput,
        read_ts: u64,
        split: bool,
    ) -> Result<(), PostingError> {
        let splits = self.plist.splits();
        let into_parts = split && !splits.is_empty();
        let mut split_idx = 0usize;
        let bounds = |idx: usize| -> (u64, u64) {
            let start = splits.get(idx).copied().unwrap_or(1);
            let end = splits.get(idx + 1).map_or(u64::MAX, |next| next - 1);
            (start, end)
        };
        let (mut start_uid, mut end_uid) = if into_parts { bounds(0) } else { (1, u64::MAX) };
        let mut enc = Encoder::new(BLOCK_SIZE);
        let mut plist = PostingList::default();
        // Postings that share a uid, literals of a quad list whose fingerprints collide, share its pack entry.
        let mut last_uid = None;
        // The bare reference at `last_uid`, kept whole only if a posting kept beside the pack shares its uid: a
        // reference's identity sorts before a literal's of the same uid.
        let mut bare_ref = None;

        self.iterate(read_ts, 0, |p| {
            // A part no posting falls in is left empty, and removed with the other empty parts.
            while into_parts && p.uid() > end_uid {
                let done = std::mem::replace(&mut enc, Encoder::new(BLOCK_SIZE));
                let mut part = std::mem::take(&mut plist);
                part.set_pack(done.done());
                out.insert_part(start_uid, part);
                split_idx += 1;
                (start_uid, end_uid) = bounds(split_idx);
            }
            if last_uid != Some(p.uid()) {
                enc.add(p.uid());
                last_uid = Some(p.uid());
                bare_ref = None;
            }
            // A posting the pack cannot carry stays whole: one with facets, statement IDs or a value.
            if !p.facets().is_empty()
                || !p.statement_ids().is_empty()
                || p.posting_type() != PostingType::Ref
            {
                plist.postings_mut().extend(bare_ref.take());
                plist.postings_mut().push(p.clone());
            } else {
                bare_ref = Some(p.clone());
            }
            Ok::<_, PostingError>(ControlFlow::Continue(()))
        })?;

        plist.set_pack(enc.done());
        if into_parts {
            out.insert_part(start_uid, plist);
        } else {
            *out.plist_mut() = plist.with_splits(out.plist().splits().to_vec());
        }
        Ok(())
    }
}
