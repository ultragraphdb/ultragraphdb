/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (pIterator). Changed for ultragraphdb: translated to Rust; the parts
 * of a split list are those the list holds, so moving to the next part reads nothing, and a part the list's splits
 * name but the list lacks is an error; a posting is keyed by 128 bits, a value's fingerprint below every reference's ID, and
 * a pack holds references alone, so a part's values come first, from its postings, then its pack's IDs.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::utils::list_keys::split_key_of;
use ultragraphdb_core_codec::{Decoder, Posting, PostingList, SeekPos};

use crate::errors::posting_error::PostingError;
use crate::types::list::List;

/// The keys below it are values' fingerprints; a reference's key is its ID, whose type prefix is never 0.
pub(crate) const VALUE_KEYS_END: u128 = 1 << 64;

/// What the iterator is at: a posting of the immutable layer, or a reference's ID the layer holds no posting for.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PPosting<'a> {
    Found(&'a Posting),
    Uid(u128),
}

/// Iterates the posting keys of a list's immutable layer after a key, part after part, with their postings: Dgraph's
/// `pIterator`. In each part the values' keys come first, from its postings, then its pack's IDs, block by block.
/// Behind a delete-all it yields nothing. An after-key of 0 starts at the first key.
pub(crate) struct PIterator<'a> {
    parts: Vec<&'a PostingList>,
    part_idx: usize,
    decoder: Option<Decoder<'a>>,
    uids: Vec<u128>,
    /// Whether `uids` holds the current part's value keys rather than one of its pack's blocks.
    values: bool,
    uidx: usize,
    pidx: usize,
    after_uid: u128,
    deleted: bool,
}

impl<'a> PIterator<'a> {
    /// The iterator over `list` after `after_uid`; `delete_below_ts`, when non-zero, is the timestamp of a delete-all
    /// above the immutable layer, which hides it.
    pub fn seek(
        list: &'a List,
        after_uid: u128,
        delete_below_ts: u64,
    ) -> Result<Self, PostingError> {
        // A rollup is stored at a commit timestamp plus one, so a transaction starting there can delete all at the
        // list's own minTs.
        if delete_below_ts > 0 && delete_below_ts < list.min_ts() {
            return Err(PostingError::delete_below_min_ts(
                delete_below_ts,
                list.min_ts(),
            ));
        }
        let splits = list.plist().splits();
        if let Some(&missing) = splits.get(list.parts().len()) {
            return Err(PostingError::missing_part(split_key_of(
                list.key(),
                missing,
            )?));
        }
        let mut it = Self {
            parts: list.immutable_parts(),
            part_idx: select_initial_split(splits, after_uid),
            decoder: None,
            uids: Vec::new(),
            values: false,
            uidx: 0,
            pidx: 0,
            after_uid,
            deleted: delete_below_ts > 0,
        };
        if !it.deleted {
            it.load_part();
        }
        Ok(it)
    }

    /// Positions the iterator at the start of the current part, after `after_uid`: its value keys, or, when it has
    /// none left, its pack's block holding the first ID after `after_uid`.
    fn load_part(&mut self) {
        let Some(part) = self.parts.get(self.part_idx).copied() else {
            return;
        };
        let after = self.after_uid;
        let later = |key: u128| after == 0 || key > after;
        self.decoder = part
            .pack()
            .map(|pack| Decoder::sought(pack, after, SeekPos::Current));
        self.uids = part
            .postings()
            .iter()
            .map(Posting::uid)
            .take_while(|key| *key < VALUE_KEYS_END)
            .filter(|key| later(*key))
            .collect();
        self.uids.dedup();
        self.values = !self.uids.is_empty();
        if !self.values {
            self.uids = self.block();
        }
        self.uidx = 0;
        self.pidx = if after == 0 {
            0
        } else {
            part.postings().partition_point(|p| p.uid() <= after)
        };
    }

    /// The keys of the decoder's current block.
    fn block(&self) -> Vec<u128> {
        self.decoder
            .as_ref()
            .map(|d| d.ids().iter().map(|id| id.to_u128()).collect())
            .unwrap_or_default()
    }

    /// Moves on to the next part holding keys, if the list is split.
    fn move_to_next_valid_part(&mut self) {
        // Ends: each turn moves to the next of the list's parts.
        while self.uids.is_empty() && self.part_idx + 1 < self.parts.len() {
            self.part_idx += 1;
            self.load_part();
        }
    }

    /// Whether the iterator is at a key.
    pub fn valid(&mut self) -> bool {
        if self.deleted {
            return false;
        }
        if self.uids.is_empty() {
            self.move_to_next_valid_part();
        }
        !self.uids.is_empty()
    }

    /// Moves to the next posting: the next one of the current key, which a quad list holds when literals'
    /// fingerprints collide, or the next key.
    pub fn next(&mut self) {
        if self.deleted {
            self.uids.clear();
            return;
        }
        if let (Some(&uid), Some(part)) = (self.uids.get(self.uidx), self.parts.get(self.part_idx))
            && part
                .postings()
                .get(self.pidx)
                .is_some_and(|p| p.uid() == uid)
            && part
                .postings()
                .get(self.pidx + 1)
                .is_some_and(|p| p.uid() == uid)
        {
            self.pidx += 1;
            return;
        }
        self.uidx += 1;
        if self.uidx < self.uids.len() {
            return;
        }
        self.uidx = 0;
        self.uids = if self.values {
            // The part's values are done; its pack's IDs follow, from the block holding the first after them.
            self.values = false;
            self.block()
        } else {
            self.decoder
                .as_mut()
                .map(|d| d.next_block().iter().map(|id| id.to_u128()).collect())
                .unwrap_or_default()
        };
        self.move_to_next_valid_part();
    }

    /// The posting at the current key, or the key alone when the part holds no posting for it. Valid only after
    /// [`PIterator::valid`] returned true.
    pub fn posting(&mut self) -> PPosting<'a> {
        let Some(&uid) = self.uids.get(self.uidx) else {
            return PPosting::Uid(0);
        };
        let Some(part) = self.parts.get(self.part_idx).copied() else {
            return PPosting::Uid(uid);
        };
        // Ends: each turn moves past one of the part's postings.
        while let Some(p) = part.postings().get(self.pidx) {
            if p.uid() > uid {
                break;
            }
            if p.uid() == uid {
                return PPosting::Found(p);
            }
            self.pidx += 1;
        }
        debug_assert!(
            uid >= VALUE_KEYS_END,
            "a value's key always has its posting"
        );
        PPosting::Uid(uid)
    }
}

/// The part holding `after_uid`: Dgraph's `selectInitialSplit`.
fn select_initial_split(splits: &[u128], after_uid: u128) -> usize {
    if after_uid == 0 {
        return 0;
    }
    for (i, &start) in splits.iter().enumerate() {
        // A part starting at after_uid is the one to read.
        if start == after_uid {
            return i;
        }
        // A part starting above after_uid: the previous one may hold keys above it.
        if start > after_uid {
            return i.saturating_sub(1);
        }
    }
    splits.len().saturating_sub(1)
}
