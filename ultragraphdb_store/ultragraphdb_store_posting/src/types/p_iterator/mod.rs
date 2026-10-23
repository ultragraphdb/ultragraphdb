/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (pIterator). Changed for ultragraphdb: translated to Rust; the parts
 * of a split list are those the list holds, so moving to the next part reads nothing, and a part the list's splits
 * name but the list lacks is an error.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_codec::{Decoder, SeekPos};
use ultragraphdb_core_keys::split_key;
use ultragraphdb_proto_pb::pb::{Posting, PostingList};

use crate::errors::posting_error::PostingError;
use crate::types::list::List;

/// What the iterator is at: a posting of the immutable layer, or a uid the layer holds no posting for.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PPosting<'a> {
    Found(&'a Posting),
    Uid(u64),
}

impl PPosting<'_> {
    pub fn uid(&self) -> u64 {
        match self {
            PPosting::Found(p) => p.uid,
            PPosting::Uid(uid) => *uid,
        }
    }
}

/// Iterates the uids of a list's immutable layer after a uid, part after part, with their postings: Dgraph's
/// `pIterator`. Behind a delete-all it yields nothing.
pub(crate) struct PIterator<'a> {
    parts: Vec<&'a PostingList>,
    part_idx: usize,
    decoder: Option<Decoder<'a>>,
    uids: Vec<u64>,
    uidx: usize,
    pidx: usize,
    after_uid: u64,
    deleted: bool,
}

impl<'a> PIterator<'a> {
    /// The iterator over `list` after `after_uid`; `delete_below_ts`, when non-zero, is the timestamp of a delete-all
    /// above the immutable layer, which hides it.
    pub fn seek(
        list: &'a List,
        after_uid: u64,
        delete_below_ts: u64,
    ) -> Result<Self, PostingError> {
        // A rollup is stored at a commit timestamp plus one, so a transaction starting there can delete all at the
        // list's own minTs.
        if delete_below_ts > 0 && delete_below_ts < list.min_ts() {
            return Err(PostingError::DeleteBelowMinTs(
                delete_below_ts,
                list.min_ts(),
            ));
        }
        let splits = &list.plist().splits;
        if list.parts().len() < splits.len() {
            let missing = splits[list.parts().len()];
            return Err(PostingError::MissingPart(split_key(list.key(), missing)?));
        }
        let mut it = Self {
            parts: list.immutable_parts(),
            part_idx: select_initial_split(splits, after_uid),
            decoder: None,
            uids: Vec::new(),
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

    /// Positions the iterator at the start of the current part, after `after_uid`.
    fn load_part(&mut self) {
        let Some(part) = self.parts.get(self.part_idx).copied() else {
            return;
        };
        self.decoder = part
            .pack
            .as_ref()
            .map(|pack| Decoder::sought(pack, self.after_uid, SeekPos::Current));
        self.uids = self
            .decoder
            .as_ref()
            .map(|d| d.uids().to_vec())
            .unwrap_or_default();
        self.uidx = 0;
        let after = self.after_uid;
        self.pidx = part.postings.partition_point(|p| p.uid <= after);
    }

    /// Moves on to the next part holding uids, if the list is split.
    fn move_to_next_valid_part(&mut self) {
        while self.uids.is_empty() && self.part_idx + 1 < self.parts.len() {
            self.part_idx += 1;
            self.load_part();
        }
    }

    /// Whether the iterator is at a uid.
    pub fn valid(&mut self) -> bool {
        if self.deleted {
            return false;
        }
        if self.uids.is_empty() {
            self.move_to_next_valid_part();
        }
        !self.uids.is_empty()
    }

    /// Moves to the next uid.
    pub fn next(&mut self) {
        if self.deleted {
            self.uids.clear();
            return;
        }
        self.uidx += 1;
        if self.uidx < self.uids.len() {
            return;
        }
        self.uidx = 0;
        self.uids = self
            .decoder
            .as_mut()
            .map(|d| d.next_block().to_vec())
            .unwrap_or_default();
        self.move_to_next_valid_part();
    }

    /// The posting at the current uid, or the uid alone when the part holds no posting for it. Valid only after
    /// [`PIterator::valid`] returned true.
    pub fn posting(&mut self) -> PPosting<'a> {
        let Some(&uid) = self.uids.get(self.uidx) else {
            return PPosting::Uid(0);
        };
        let Some(part) = self.parts.get(self.part_idx).copied() else {
            return PPosting::Uid(uid);
        };
        while let Some(p) = part.postings.get(self.pidx) {
            if p.uid > uid {
                break;
            }
            if p.uid == uid {
                return PPosting::Found(p);
            }
            self.pidx += 1;
        }
        PPosting::Uid(uid)
    }
}

/// The part holding `after_uid`: Dgraph's `selectInitialSplit`.
fn select_initial_split(splits: &[u64], after_uid: u64) -> usize {
    if after_uid == 0 {
        return 0;
    }
    for (i, &start) in splits.iter().enumerate() {
        // A part starting at after_uid is the one to read.
        if start == after_uid {
            return i;
        }
        // A part starting above after_uid: the previous one may hold uids above it.
        if start > after_uid {
            return i.saturating_sub(1);
        }
    }
    splits.len().saturating_sub(1)
}
