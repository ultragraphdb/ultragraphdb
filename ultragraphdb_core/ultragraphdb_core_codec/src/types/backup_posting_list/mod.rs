/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, protos/pb.proto (BackupPostingList). Changed for ultragraphdb: a Rust type in place of
 * the protobuf message.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_ids::SystemId;

use crate::types::posting::Posting;

/// A posting list as a backup holds it: Dgraph's `BackupPostingList`, its IDs plain or as LEB128 deltas of their
/// 128-bit values in `uid_bytes`, beside the postings, the commit timestamp and the keys its parts start at.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackupPostingList {
    uids: Vec<SystemId>,
    postings: Vec<Posting>,
    commit_ts: u64,
    splits: Vec<u128>,
    uid_bytes: Vec<u8>,
}

impl BackupPostingList {
    /// The IDs as a plain list; when given, `uid_bytes` is not read.
    #[must_use]
    pub fn uids(&self) -> &[SystemId] {
        &self.uids
    }

    /// The postings the IDs cannot carry.
    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }

    /// The commit timestamp of the list.
    pub fn commit_ts(&self) -> u64 {
        self.commit_ts
    }

    /// The keys a split list's parts start at; empty for a list that is not split.
    #[must_use]
    pub fn splits(&self) -> &[u128] {
        &self.splits
    }

    /// The IDs as LEB128 deltas of their 128-bit values, each from the previous ID, the first from 0.
    pub fn uid_bytes(&self) -> &[u8] {
        &self.uid_bytes
    }

    /// The list with `uids` as its plain IDs.
    #[must_use]
    pub fn with_uids(self, uids: Vec<SystemId>) -> Self {
        Self { uids, ..self }
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

    /// The list with `uid_bytes` as its IDs in LEB128 deltas.
    pub fn with_uid_bytes(self, uid_bytes: Vec<u8>) -> Self {
        Self { uid_bytes, ..self }
    }
}
