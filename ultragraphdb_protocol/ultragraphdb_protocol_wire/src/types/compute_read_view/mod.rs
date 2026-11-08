/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod compute_read_view_body_view;

use xxhash_rust::xxh3::xxh3_64;

use crate::types::quad_pattern::QuadPattern;
use crate::types::read_level::ReadLevel;
use crate::types::term_table_view::TermTableView;

/// The body of a received ComputeRead frame, read in place; see [`crate::ComputeRead`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeReadView<'a> {
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
    max_frame: u32,
    credit: u32,
    terms: TermTableView<'a>,
    pattern: QuadPattern,
    resume: &'a [u8],
    pattern_sections: &'a [u8],
}

impl<'a> ComputeReadView<'a> {
    /// The pattern's hash a resume key holds: XXH3-64, seed 0, of the body's sections 0 to 2 (`PROTOCOL.md` §11.8).
    #[must_use]
    pub fn pattern_hash(&self) -> u64 {
        xxh3_64(self.pattern_sections)
    }

    /// The transaction's partition; 0 for none.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// Whether the transaction is read-only.
    #[must_use]
    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// The transaction's level.
    #[must_use]
    pub fn level(&self) -> ReadLevel {
        self.level
    }

    /// Whether `read_version` is a version the client named.
    #[must_use]
    pub fn named(&self) -> bool {
        self.named
    }

    /// The transaction's tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The version the read reads at; 0 for none.
    #[must_use]
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The epoch the read waits for; 0 for none.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The client's principal.
    #[must_use]
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// A read-write transaction's identity; all zero for a read-only one.
    #[must_use]
    pub fn transaction(&self) -> [u8; 16] {
        self.transaction
    }

    /// The most rows returned; 0 for no limit.
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The milliseconds left until the transaction's deadline.
    #[must_use]
    pub fn deadline_ms(&self) -> u32 {
        self.deadline_ms
    }

    /// The largest Result frame the read takes, 524,288 to 1,048,576.
    #[must_use]
    pub fn max_frame(&self) -> u32 {
        self.max_frame
    }

    /// The read's stream's starting credit, at least 1.
    #[must_use]
    pub fn credit(&self) -> u32 {
        self.credit
    }

    /// The request's term table.
    #[must_use]
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    /// The quad pattern.
    #[must_use]
    pub fn pattern(&self) -> &QuadPattern {
        &self.pattern
    }

    /// The client's resume key; empty to start.
    #[must_use]
    pub fn resume(&self) -> &'a [u8] {
        self.resume
    }
}
