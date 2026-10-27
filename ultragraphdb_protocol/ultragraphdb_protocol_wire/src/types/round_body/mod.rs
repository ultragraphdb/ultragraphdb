/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod round_body_body_builder;

use crate::types::decision::Decision;
use crate::types::round_header::RoundHeader;
use crate::utils::sections::as_u32;

/// The size of a decision.
pub(crate) const DECISION_SIZE: usize = 16;

/// The size of a record table entry.
pub(crate) const RECORD_ENTRY_SIZE: usize = 8;

/// The Round frame's body, the log record of a round: each transaction's decision in round order, and the write-set
/// record of each committed one, unchanged as its worker built it. The committed transaction k of the round has
/// commit version `first_version + 2k`. The command header is [`RoundHeader`], its transaction count the number of
/// decisions.
///
/// | Section | Contents |
/// |---|---|
/// | 0 | decisions, 16 bytes each: `outcome` u16 (0 committed, else the [`crate::ErrorCode`] of the abort), 2 zero bytes, `record` u32 (the committed transaction's record, numbered in order; [`crate::ABSENT`] for an abort), `detail` u64 (zero for a commit) |
/// | 1 | the record table: `offset` u32 and `len` u32 of each record within section 2, record 0 at 0 and each later one at the end of the one before rounded up to 8, the padding zero |
/// | 2 | the records, each a [`crate::WriteSetRecord`] |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundBody {
    position: u64,
    parent: u64,
    incarnation: u64,
    first_version: u64,
    decisions: Vec<Decision>,
    records: Vec<Vec<u8>>,
}

impl RoundBody {
    /// An empty round at `position` of the log of `incarnation`, after the round whose checksum is `parent`, its first
    /// commit version `first_version`.
    pub fn new(position: u64, parent: u64, incarnation: u64, first_version: u64) -> Self {
        Self {
            position,
            parent,
            incarnation,
            first_version,
            decisions: Vec::new(),
            records: Vec::new(),
        }
    }

    /// Appends a committed transaction and its encoded write-set record.
    pub fn commit(&mut self, record: Vec<u8>) {
        self.decisions.push(Decision::Committed);
        self.records.push(record);
    }

    /// Appends an aborted transaction with its reason.
    pub fn abort(&mut self, code: crate::ErrorCode, detail: u64) {
        self.decisions.push(Decision::Aborted { code, detail });
    }

    /// The command header: the round's fields and its transaction count.
    pub fn header(&self) -> RoundHeader {
        RoundHeader::new(
            self.position,
            self.parent,
            self.incarnation,
            self.first_version,
            as_u32(self.decisions.len()),
        )
    }

    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }

    /// The encoded write-set records of the committed transactions, in order.
    pub fn records(&self) -> &[Vec<u8>] {
        &self.records
    }
}
