/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod round_view_body_view;

use std::ops::Range;

use crate::types::command::Command;
use crate::types::decision::Decision;
use crate::types::round_body::{DECISION_SIZE, RECORD_ENTRY_SIZE};
use crate::types::round_header::RoundHeader;
use crate::types::write_set_record_view::WriteSetRecordView;
use crate::utils::sections::{Sections, u32_at};

/// The body of a received Round frame, read in place; see [`crate::RoundBody`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundView<'a> {
    header: RoundHeader,
    decisions: Vec<Decision>,
    records: Vec<WriteSetRecordView<'a>>,
}

impl<'a> RoundView<'a> {
    pub fn header(&self) -> &RoundHeader {
        &self.header
    }

    /// Each transaction's decision, in round order.
    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }

    /// The committed transactions' records, in order.
    pub fn records(&self) -> &[WriteSetRecordView<'a>] {
        &self.records
    }

    /// The commit version of committed transaction `record`: `first_version + 2 × record`; `None` past the last.
    pub fn commit_version(&self, record: usize) -> Option<u64> {
        if record >= self.records.len() {
            return None;
        }
        self.header
            .first_version()
            .checked_add(2u64.checked_mul(record as u64)?)
    }

    /// The transactions, by their place in the round, whose write-set records overlap `range` of `body`, a Round
    /// body's bytes before its chunk table, which need not pass its checks: how a damaged chunk is named by the
    /// transactions it holds. Empty when the body's table or decisions cannot be read.
    pub fn transactions_overlapping(body: &[u8], range: Range<usize>) -> Vec<u32> {
        let Ok(sections) = Sections::<3>::read(Command::Round.number(), body, 0) else {
            return Vec::new();
        };
        let committed = sections
            .get(0)
            .as_chunks::<DECISION_SIZE>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, decision)| decision.get(..2) == Some(&[0, 0][..]))
            .map(|(transaction, _)| transaction as u32);
        let base = sections.offset(2);
        sections
            .get(1)
            .as_chunks::<RECORD_ENTRY_SIZE>()
            .0
            .iter()
            .zip(committed)
            .filter(|(entry, _)| {
                let offset = u32_at(entry.as_slice(), 0).unwrap_or(0) as usize;
                let len = u32_at(entry.as_slice(), 1).unwrap_or(0) as usize;
                let start = base.saturating_add(offset);
                let end = start.saturating_add(len);
                start < range.end && range.start < end
            })
            .map(|(_, transaction)| transaction)
            .collect()
    }
}
