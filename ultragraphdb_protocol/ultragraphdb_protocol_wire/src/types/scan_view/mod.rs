/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod scan_view_body_view;

use crate::types::scan_at::ScanAt;
use crate::types::scan_range::ScanRange;
use crate::types::term_table_view::TermTableView;
use crate::utils::constants::SCAN_RANGE_SIZE;
use crate::utils::le::bytes_at;

/// The body of a received Scan frame, read in place; see [`crate::Scan`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanView<'a> {
    at: ScanAt,
    read_version: u64,
    epoch: u64,
    principal: u64,
    transaction: [u8; 16],
    limit: u32,
    ranges: &'a [u8],
    keys: &'a [u8],
    terms: TermTableView<'a>,
}

impl<'a> ScanView<'a> {
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

    /// The number of ranges, at least 1.
    pub fn range_count(&self) -> usize {
        self.ranges.len() / SCAN_RANGE_SIZE
    }

    /// Range `index`, its keys borrowed from the frame; `None` past the last.
    pub fn range(&self, index: usize) -> Option<ScanRange<'a>> {
        let record = self
            .ranges
            .get(index.checked_mul(SCAN_RANGE_SIZE)?..)?
            .get(..SCAN_RANGE_SIZE)?;
        let field = |at| bytes_at(record, at).map(|b| u32::from_le_bytes(b) as usize);
        let (first, past) = (field(8)?, field(16)?);
        Some(ScanRange::new(
            bytes_at(record, 0).map(u32::from_le_bytes)?,
            self.keys.get(first..past)?,
            self.keys.get(past..past.checked_add(field(20)?)?)?,
        ))
    }

    /// The ranges, in order.
    pub fn ranges(&self) -> impl Iterator<Item = ScanRange<'a>> + '_ {
        (0..self.range_count()).filter_map(|i| self.range(i))
    }

    /// The term table of the IRIs the read names, at most as many terms as ranges; empty with no transaction.
    pub fn terms(&self) -> TermTableView<'a> {
        self.terms
    }
}
