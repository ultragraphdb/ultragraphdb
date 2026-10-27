/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, protos/pb.proto (BackupPostingList). Changed for ultragraphdb: a Rust type in place of
 * the protobuf message.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::posting::Posting;

/// A posting list as a backup holds it: Dgraph's `BackupPostingList`, its UIDs plain or as uvarint deltas in
/// `uid_bytes`, beside the postings, the commit timestamp and the splits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackupPostingList {
    uids: Vec<u64>,
    postings: Vec<Posting>,
    commit_ts: u64,
    splits: Vec<u64>,
    uid_bytes: Vec<u8>,
}

impl BackupPostingList {
    /// The UIDs as a plain list; when given, `uid_bytes` is not read.
    pub fn uids(&self) -> &[u64] {
        &self.uids
    }

    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }

    pub fn commit_ts(&self) -> u64 {
        self.commit_ts
    }

    pub fn splits(&self) -> &[u64] {
        &self.splits
    }

    /// The UIDs as uvarint deltas, each from the previous UID, the first from 0.
    pub fn uid_bytes(&self) -> &[u8] {
        &self.uid_bytes
    }

    pub fn with_uids(self, uids: Vec<u64>) -> Self {
        Self { uids, ..self }
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

    pub fn with_uid_bytes(self, uid_bytes: Vec<u8>) -> Self {
        Self { uid_bytes, ..self }
    }
}
