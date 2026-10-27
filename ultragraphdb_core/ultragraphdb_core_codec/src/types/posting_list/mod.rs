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

/// A posting list as stored: Dgraph's `PostingList`. A complete list holds its UIDs in the pack and, beside them, the
/// postings the pack cannot carry; a delta holds postings alone; a split list holds the start UIDs of its parts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PostingList {
    pack: Option<UidPack>,
    postings: Vec<Posting>,
    commit_ts: u64,
    splits: Vec<u64>,
}

impl PostingList {
    /// The packed UIDs, `None` when the list has no pack.
    pub fn pack(&self) -> Option<&UidPack> {
        self.pack.as_ref()
    }

    /// The postings, sorted by UID in a complete list.
    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }

    /// The commit timestamp of a delta.
    pub fn commit_ts(&self) -> u64 {
        self.commit_ts
    }

    /// The start UIDs of a split list's parts, ascending from 1; empty for a list that is not split.
    pub fn splits(&self) -> &[u64] {
        &self.splits
    }

    pub fn with_pack(self, pack: UidPack) -> Self {
        Self {
            pack: Some(pack),
            ..self
        }
    }

    pub fn with_postings(self, postings: Vec<Posting>) -> Self {
        Self { postings, ..self }
    }

    pub fn with_commit_ts(self, commit_ts: u64) -> Self {
        Self { commit_ts, ..self }
    }

    pub fn with_splits(self, splits: Vec<u64>) -> Self {
        Self { splits, ..self }
    }

    pub fn set_pack(&mut self, pack: Option<UidPack>) {
        self.pack = pack;
    }

    pub fn postings_mut(&mut self) -> &mut Vec<Posting> {
        &mut self.postings
    }

    pub fn set_commit_ts(&mut self, commit_ts: u64) {
        self.commit_ts = commit_ts;
    }

    pub fn set_splits(&mut self, splits: Vec<u64>) {
        self.splits = splits;
    }
}
