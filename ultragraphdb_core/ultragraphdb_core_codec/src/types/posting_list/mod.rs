/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, protos/pb.proto (PostingList). Changed for ultragraphdb: a Rust type in place of the
 * protobuf message, stored in the repository's posting format.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod posting_list_codec;

use crate::types::posting::Posting;
use crate::types::uid_pack::UidPack;

/// A posting list as stored: Dgraph's `PostingList`. A complete list holds its references' system IDs in the pack and,
/// beside them, the postings the pack cannot carry; a delta holds postings alone; a split list holds the 128-bit keys
/// its parts start at, as its postings are keyed (see [`Posting::uid`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PostingList {
    pack: Option<UidPack>,
    postings: Vec<Posting>,
    commit_ts: u64,
    splits: Vec<u128>,
}

impl PostingList {
    /// The packed IDs, `None` when the list has no pack.
    pub fn pack(&self) -> Option<&UidPack> {
        self.pack.as_ref()
    }

    /// The postings, sorted by key in a complete list.
    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }

    /// The commit timestamp of a delta.
    pub fn commit_ts(&self) -> u64 {
        self.commit_ts
    }

    /// The keys a split list's parts start at, ascending; empty for a list that is not split.
    #[must_use]
    pub fn splits(&self) -> &[u128] {
        &self.splits
    }

    /// The list with `pack` as its packed IDs.
    pub fn with_pack(self, pack: UidPack) -> Self {
        Self {
            pack: Some(pack),
            ..self
        }
    }

    /// The list with `postings` as its postings.
    pub fn with_postings(self, postings: Vec<Posting>) -> Self {
        Self { postings, ..self }
    }

    /// The list with `commit_ts` as its commit timestamp.
    pub fn with_commit_ts(self, commit_ts: u64) -> Self {
        Self { commit_ts, ..self }
    }

    /// The list with `splits` as the keys its parts start at.
    #[must_use]
    pub fn with_splits(self, splits: Vec<u128>) -> Self {
        Self { splits, ..self }
    }

    /// Sets the packed IDs to `pack`; `None` removes the pack.
    pub fn set_pack(&mut self, pack: Option<UidPack>) {
        self.pack = pack;
    }

    /// The postings, to edit in place.
    pub fn postings_mut(&mut self) -> &mut Vec<Posting> {
        &mut self.postings
    }

    /// Sets the commit timestamp to `commit_ts`.
    pub fn set_commit_ts(&mut self, commit_ts: u64) {
        self.commit_ts = commit_ts;
    }

    /// Sets the keys its parts start at to `splits`.
    pub fn set_splits(&mut self, splits: Vec<u128>) {
        self.splits = splits;
    }
}
