/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod scan_body_builder;

use crate::types::scan_at::ScanAt;
use crate::types::scan_range::ScanRange;
use crate::types::term_table::TermTable;

/// A range as a Scan's builder holds it, its keys copied.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnedRange {
    partition: u32,
    first: Vec<u8>,
    past: Vec<u8>,
    after: Vec<u8>,
}

/// Storage's read kernel (`PROTOCOL.md` §13.13): the entries of key ranges of the partition at a role address, each
/// range at a version, at most `limit` entries of it, in key order, batched per address. Answered with Entries frames.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `at` | u8, a [`ScanAt`] |
/// | 8 | `read_version` | u64: with `at` 1 and 4, the version; 0 otherwise |
/// | 16 | `epoch` | u64: the epoch the partition closes before it answers; 0 for none, for `at` 4 and with a transaction |
/// | 24 | `principal` | u64: the client's principal |
/// | 32 | `transaction` | 16 bytes: a read-write transaction's identity, the partition's own, whose reads record the ranges; all zero for none |
/// | 48 | `limit` | u32: the most entries per range; 0 for no limit |
/// | 52 | `deadline_ms` | u32: how long the partition waits for the close of `epoch`, the time left until the deadline of the read's transaction; 0 when `epoch` is 0 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | ranges, 24 bytes each ([`ScanRange::encode`]), at least one |
/// | 1 | the ranges' keys, back to back: each range's first key, its past key, then its `after` when it has one |
/// | 2 | with `transaction` not zero, the IRIs the read names, each an entry of kind 1: a [`TermTable`]'s records, at most as many as ranges, interned in the order of the ranges naming them; empty otherwise |
/// | 3 | that term table's text |
///
/// A Scan naming a transaction sets `at` 1 and `epoch` 0, names each IRI the read needs an ID for by its text, and its
/// first ranges name, in the table's order, one range each holding that IRI's dictionary key alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    at: ScanAt,
    read_version: u64,
    epoch: u64,
    principal: u64,
    transaction: [u8; 16],
    limit: u32,
    deadline_ms: u32,
    ranges: Vec<OwnedRange>,
    terms: TermTable,
}

impl Scan {
    /// A Scan reading `at` `read_version` (0 where `at` names none), once `epoch` has closed, for `principal`, in
    /// `transaction` (all zero for none), at most `limit` entries per range, waiting at most `deadline_ms` for that close
    /// (0 when `epoch` is 0); no range and no term yet.
    #[must_use]
    pub fn new(
        at: ScanAt,
        read_version: u64,
        epoch: u64,
        principal: u64,
        transaction: [u8; 16],
        limit: u32,
        deadline_ms: u32,
    ) -> Self {
        Self {
            at,
            read_version,
            epoch,
            principal,
            transaction,
            limit,
            deadline_ms,
            ranges: Vec::new(),
            terms: TermTable::new(),
        }
    }

    /// The Scan with `terms` as its term table, the IRIs a transaction's read names.
    #[must_use]
    pub fn with_terms(mut self, terms: TermTable) -> Self {
        self.terms = terms;
        self
    }

    /// Appends `range`, its keys copied.
    pub fn push_range(&mut self, range: ScanRange<'_>) {
        self.ranges.push(OwnedRange {
            partition: range.partition(),
            first: range.first().to_vec(),
            past: range.past().to_vec(),
            after: range.after().to_vec(),
        });
    }

    /// The version each range is read at.
    #[must_use]
    pub fn at(&self) -> ScanAt {
        self.at
    }

    /// The version named; 0 where `at` names none.
    #[must_use]
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The epoch the partition closes before it answers; 0 for none.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The client's principal.
    #[must_use]
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The read-write transaction's identity; all zero for none.
    #[must_use]
    pub fn transaction(&self) -> [u8; 16] {
        self.transaction
    }

    /// The most entries per range; 0 for no limit.
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// How long the partition waits for the close of the epoch, in milliseconds; 0 when the epoch is 0.
    #[must_use]
    pub fn deadline_ms(&self) -> u32 {
        self.deadline_ms
    }

    /// The ranges, in order.
    pub fn ranges(&self) -> impl Iterator<Item = ScanRange<'_>> {
        self.ranges.iter().map(|range| {
            ScanRange::new(range.partition, &range.first, &range.past).with_after(&range.after)
        })
    }

    /// The term table of the IRIs the read names.
    #[must_use]
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }
}
