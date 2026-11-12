/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod round_view_body_view;

use std::ops::Range;
use ultragraphdb_core_body::u32_at;

use crate::types::command::Command;
use crate::types::decision::Decision;
use crate::types::key_values::KeyValues;
use crate::types::round_body::{DECISION_SIZE, RECORD_ENTRY_SIZE};
use crate::types::round_header::RoundHeader;
use crate::types::write_set_record_view::WriteSetRecordView;
use crate::utils::body::read_sections;

/// The body of a received Round frame, read in place; see [`crate::RoundBody`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundView<'a> {
    header: RoundHeader,
    decisions: Vec<Decision>,
    records: Vec<WriteSetRecordView<'a>>,
    dictionary: KeyValues<'a>,
}

impl<'a> RoundView<'a> {
    /// The round's header.
    pub fn header(&self) -> &RoundHeader {
        &self.header
    }

    /// Each entry's decision, in round order.
    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }

    /// The committed entries' records, in order.
    pub fn records(&self) -> &[WriteSetRecordView<'a>] {
        &self.records
    }

    /// The dictionary entries the round creates, in ascending key order.
    #[must_use]
    pub fn dictionary(&self) -> KeyValues<'a> {
        self.dictionary
    }

    /// The commit version of committed entry `record`: `first_version + 2 × record`; `None` past the last.
    pub fn commit_version(&self, record: usize) -> Option<u64> {
        if record >= self.records.len() {
            return None;
        }
        self.header
            .first_version()
            .checked_add(2u64.checked_mul(record as u64)?)
    }

    /// The entries, by their place in the round, whose write-set records overlap `range` of `body`, a Round body's
    /// bytes before its chunk table, which need not pass its checks: how a damaged chunk is named by the entries it
    /// holds. Empty when the body's table or decisions cannot be read.
    pub fn transactions_overlapping(body: &[u8], range: Range<usize>) -> Vec<u32> {
        let Ok(sections) = read_sections::<4>(Command::Round.number(), body, 0) else {
            return Vec::new();
        };
        let committed = sections
            .get(0)
            .as_chunks::<DECISION_SIZE>()
            .0
            .iter()
            // Numbered as u32s: a frame holds far fewer decisions than `u32::MAX`.
            .zip(0..=u32::MAX)
            .filter(|(decision, _)| decision.get(..2) == Some(&[0, 0][..]))
            .map(|(_, transaction)| transaction);
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
