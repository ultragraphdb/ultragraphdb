/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, protos/pb.proto (Posting). Changed for ultragraphdb: a Rust type in place of the
 * protobuf message, stored in the repository's posting format; `val_type`, which no code reads, is not carried; the
 * statement IDs are the quad layout's.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::facet::Facet;
use crate::types::posting_type::PostingType;

/// One object of a posting list: Dgraph's `Posting`. A reference is its UID; a value posting holds its value, ordered
/// by its UID. `op` is the write's operation, and the start and commit timestamps are those of the transaction that
/// wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Posting {
    uid: u64,
    value: Vec<u8>,
    posting_type: PostingType,
    lang_tag: Vec<u8>,
    facets: Vec<Facet>,
    op: u32,
    start_ts: u64,
    commit_ts: u64,
    statement_ids: Vec<u64>,
}

impl Posting {
    pub fn uid(&self) -> u64 {
        self.uid
    }

    pub fn value(&self) -> &[u8] {
        &self.value
    }

    pub fn posting_type(&self) -> PostingType {
        self.posting_type
    }

    /// The value's language, set only for a language-tagged value.
    pub fn lang_tag(&self) -> &[u8] {
        &self.lang_tag
    }

    pub fn facets(&self) -> &[Facet] {
        &self.facets
    }

    pub fn op(&self) -> u32 {
        self.op
    }

    pub fn start_ts(&self) -> u64 {
        self.start_ts
    }

    pub fn commit_ts(&self) -> u64 {
        self.commit_ts
    }

    /// The IDs of the statements holding the object, for parallel edges and statements with properties.
    pub fn statement_ids(&self) -> &[u64] {
        &self.statement_ids
    }

    pub fn with_uid(self, uid: u64) -> Self {
        Self { uid, ..self }
    }

    pub fn with_value(self, value: Vec<u8>) -> Self {
        Self { value, ..self }
    }

    pub fn with_posting_type(self, posting_type: PostingType) -> Self {
        Self {
            posting_type,
            ..self
        }
    }

    pub fn with_lang_tag(self, lang_tag: Vec<u8>) -> Self {
        Self { lang_tag, ..self }
    }

    pub fn with_facets(self, facets: Vec<Facet>) -> Self {
        Self { facets, ..self }
    }

    pub fn with_op(self, op: u32) -> Self {
        Self { op, ..self }
    }

    pub fn with_start_ts(self, start_ts: u64) -> Self {
        Self { start_ts, ..self }
    }

    pub fn with_commit_ts(self, commit_ts: u64) -> Self {
        Self { commit_ts, ..self }
    }

    pub fn with_statement_ids(self, statement_ids: Vec<u64>) -> Self {
        Self {
            statement_ids,
            ..self
        }
    }

    pub fn set_op(&mut self, op: u32) {
        self.op = op;
    }

    pub fn set_start_ts(&mut self, start_ts: u64) {
        self.start_ts = start_ts;
    }

    pub fn set_commit_ts(&mut self, commit_ts: u64) {
        self.commit_ts = commit_ts;
    }
}
