/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod compute_read_body_builder;

use crate::types::quad_pattern::QuadPattern;
use crate::types::read_level::ReadLevel;
use crate::types::term_table::TermTable;

/// The frontend sends a client's read to compute (`PROTOCOL.md` §13.12): a Read, or an Execute with its parameters
/// bound into the pattern, with what compute needs of the read's transaction. Answered with Result frames, each
/// batch's `epoch` and `partition` filled, or an Error.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `partition` | u32: the transaction's partition once bound; 0 for none |
/// | 4 | `mode` | u8: 0 read-write, 1 read-only |
/// | 5 | `level` | u8, a [`ReadLevel`] |
/// | 6 | `named` | u8 boolean: `read_version` is a version the client named; only read-only at level 0 |
/// | 8 | `tenant` | u64 |
/// | 16 | `read_version` | u64: the version the read reads at, or 0 |
/// | 24 | `epoch` | u64: the snapshot's epoch, the token's, or the read's floor; 0 for none |
/// | 32 | `principal` | u64: the client's principal |
/// | 40 | `transaction` | 16 bytes: a read-write transaction's identity; all zero for a read-only one |
/// | 56 | `limit` | u32: the most rows; 0 for no limit |
/// | 60 | `deadline_ms` | u32: the milliseconds left until the transaction's deadline |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the pattern: four [`crate::PatternSlot`]s, no parameter |
/// | 1 | the term table's records |
/// | 2 | the term table's text |
/// | 3 | the client's resume key |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeRead {
    partition: u32,
    read_only: bool,
    level: ReadLevel,
    named: bool,
    tenant: u64,
    read_version: u64,
    epoch: u64,
    principal: u64,
    transaction: [u8; 16],
    limit: u32,
    deadline_ms: u32,
    terms: TermTable,
    pattern: QuadPattern,
    resume: Vec<u8>,
}

impl ComputeRead {
    /// A read of `pattern`, naming `terms`, from `resume`, in a transaction of `tenant` at `level`, read-only or
    /// read-write; every other field 0 until set.
    pub fn new(
        tenant: u64,
        read_only: bool,
        level: ReadLevel,
        terms: TermTable,
        pattern: QuadPattern,
        resume: Vec<u8>,
    ) -> Self {
        Self {
            partition: 0,
            read_only,
            level,
            named: false,
            tenant,
            read_version: 0,
            epoch: 0,
            principal: 0,
            transaction: [0; 16],
            limit: 0,
            deadline_ms: 0,
            terms,
            pattern,
            resume,
        }
    }

    /// The transaction's partition.
    pub fn with_partition(mut self, partition: u32) -> Self {
        self.partition = partition;
        self
    }

    /// A read-write transaction's identity, as its Bind named it.
    pub fn with_transaction(mut self, transaction: [u8; 16]) -> Self {
        self.transaction = transaction;
        self
    }

    /// The version the read reads at: the Bind's answer, or the snapshot's version bound.
    pub fn with_read_version(mut self, read_version: u64) -> Self {
        self.read_version = read_version;
        self.named = false;
        self
    }

    /// The version the client named, which the read reads at without waiting.
    pub fn with_named_version(mut self, read_version: u64) -> Self {
        self.read_version = read_version;
        self.named = true;
        self
    }

    /// The epoch the read waits for: the snapshot's, the token's, or the read's floor.
    pub fn with_epoch(mut self, epoch: u64) -> Self {
        self.epoch = epoch;
        self
    }

    /// The client's principal.
    pub fn with_principal(mut self, principal: u64) -> Self {
        self.principal = principal;
        self
    }

    /// The most rows returned; 0 for no limit.
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// The milliseconds left until the transaction's deadline.
    pub fn with_deadline_ms(mut self, deadline_ms: u32) -> Self {
        self.deadline_ms = deadline_ms;
        self
    }

    /// The transaction's partition; 0 for none.
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// Whether the transaction is read-only.
    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// The transaction's level.
    pub fn level(&self) -> ReadLevel {
        self.level
    }

    /// Whether `read_version` is a version the client named.
    pub fn named(&self) -> bool {
        self.named
    }

    /// The transaction's tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The version the read reads at; 0 for none.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The epoch the read waits for; 0 for none.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The client's principal.
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// A read-write transaction's identity; all zero for a read-only one.
    pub fn transaction(&self) -> [u8; 16] {
        self.transaction
    }

    /// The most rows returned; 0 for no limit.
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The milliseconds left until the transaction's deadline.
    pub fn deadline_ms(&self) -> u32 {
        self.deadline_ms
    }

    /// The request's term table.
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    /// The quad pattern.
    pub fn pattern(&self) -> &QuadPattern {
        &self.pattern
    }

    /// The client's resume key; empty to start.
    pub fn resume(&self) -> &[u8] {
        &self.resume
    }
}
