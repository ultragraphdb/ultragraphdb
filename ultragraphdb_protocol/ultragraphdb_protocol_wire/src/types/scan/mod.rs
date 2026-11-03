/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod scan_body_builder;

use crate::types::scan_at::ScanAt;
use crate::types::scan_range::ScanRange;

/// Storage's read kernel (`PROTOCOL.md` §13.13): the entries of key ranges of the partitions one storage process
/// hosts, each range at a version, at most `limit` entries of it, in key order. Answered with Entries frames.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `at` | u8, a [`ScanAt`] |
/// | 8 | `read_version` | u64: with `at` 1 and 4, the version; 0 otherwise |
/// | 16 | `epoch` | u64: the epoch the partition closes before it answers; 0 for none, and for `at` 4 |
/// | 24 | `principal` | u64: the client's principal |
/// | 32 | `transaction` | 16 bytes: a read-write transaction's identity, whose reads record the ranges; all zero for none |
/// | 48 | `limit` | u32: the most entries per range; 0 for no limit |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | ranges, 24 bytes each ([`ScanRange::encode`]), at least one |
/// | 1 | the ranges' keys, back to back: each range's first key, then its past key |
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scan {
    at: ScanAt,
    read_version: u64,
    epoch: u64,
    principal: u64,
    transaction: [u8; 16],
    limit: u32,
    ranges: Vec<(u32, Vec<u8>, Vec<u8>)>,
}

impl Scan {
    /// A Scan reading `at` `read_version` (0 where `at` names none), once `epoch` has closed, for `principal`, in
    /// `transaction` (all zero for none), at most `limit` entries per range; no range yet.
    pub fn new(
        at: ScanAt,
        read_version: u64,
        epoch: u64,
        principal: u64,
        transaction: [u8; 16],
        limit: u32,
    ) -> Self {
        Self {
            at,
            read_version,
            epoch,
            principal,
            transaction,
            limit,
            ranges: Vec::new(),
        }
    }

    /// Appends `range`, its keys copied.
    pub fn push_range(&mut self, range: ScanRange<'_>) {
        self.ranges.push((
            range.partition(),
            range.first().to_vec(),
            range.past().to_vec(),
        ));
    }

    /// The version each range is read at.
    pub fn at(&self) -> ScanAt {
        self.at
    }

    /// The version named; 0 where `at` names none.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The epoch the partition closes before it answers; 0 for none.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The client's principal.
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The read-write transaction's identity; all zero for none.
    pub fn transaction(&self) -> [u8; 16] {
        self.transaction
    }

    /// The most entries per range; 0 for no limit.
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The ranges, in order.
    pub fn ranges(&self) -> impl Iterator<Item = ScanRange<'_>> {
        self.ranges
            .iter()
            .map(|(partition, first, past)| ScanRange::new(*partition, first, past))
    }
}
