/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{as_u32, u32_at};
use ultragraphdb_error_protocol::ProtocolError;
use xxhash_rust::xxh3::xxh3_64;

use crate::types::command::Command;
use crate::types::resume_key::{RESUME_FIXED, RESUME_PARTITION, RESUME_ZERO};
use crate::types::resume_row::ResumeRow;
use crate::types::term_table_view::TermTableView;
use crate::utils::body::read_sections;
use crate::utils::constants::ABSENT;
use crate::utils::le::bytes_at;

/// The bytes of a resume key's last row: five u32 indices.
const RESUME_ROW: usize = 20;

// `partition` is read as the fixed part's u32 of index `RESUME_PARTITION / 4`, exactly.
const _: () = assert!(RESUME_PARTITION.is_multiple_of(4));

/// The row's position `at` in the key's table of `terms` entries: its index, or `None` where `absent` allows [`ABSENT`].
fn position(
    row: &[u8],
    at: usize,
    terms: usize,
    absent: bool,
) -> Result<Option<u32>, ProtocolError> {
    let command = Command::ComputeRead.number();
    let index = u32_at(row, at).unwrap_or(ABSENT);
    if absent && index == ABSENT {
        return Ok(None);
    }
    // Lossless: a `usize` is at least 32 bits.
    if index as usize >= terms {
        return Err(ProtocolError::index_out_of_range(
            command,
            0,
            at,
            index,
            as_u32(terms),
        ));
    }
    Ok(Some(index))
}

/// A resume key a client sent back, read in place (`PROTOCOL.md` §11.8) and checked in the section's order up to its term
/// indices; the pattern hash and the `scope` are the caller's to compare, in that order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeKeyView<'a> {
    pattern: u64,
    scope: u64,
    partition: u32,
    ordering: &'a [u8],
    terms: TermTableView<'a>,
    row: ResumeRow,
}

impl<'a> ResumeKeyView<'a> {
    /// The key `bytes`, checked once.
    ///
    /// # Errors
    ///
    /// In order: its checksum ([`ProtocolError::field_value`] `"resume key"`); the body layout; bytes 28 to 31 zero
    /// ([`ProtocolError::padding_not_zero`]); `partition` at least 1 ([`ProtocolError::field_value`] `"partition"`); section 0 of 20 bytes
    /// ([`ProtocolError::count_mismatch`] `"resume row"`); the term table; each index below the table's length,
    /// `ABSENT` only for the graph and the reifier ([`ProtocolError::index_out_of_range`]).
    pub fn read(bytes: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::ComputeRead.number();
        let checksum = bytes_at(bytes, 0).map(u64::from_le_bytes);
        if checksum != Some(xxh3_64(bytes.get(8..).unwrap_or_default())) {
            return Err(ProtocolError::field_value(command, "resume key", 0));
        }
        let sections = read_sections::<4>(command, bytes, RESUME_FIXED)?;
        let fixed = sections.fixed();
        let zero = fixed.get(RESUME_ZERO..RESUME_FIXED).unwrap_or_default();
        if let Some(at) = zero.iter().position(|byte| *byte != 0) {
            return Err(ProtocolError::padding_not_zero(
                command,
                RESUME_ZERO.saturating_add(at),
            ));
        }
        let partition = u32_at(fixed, RESUME_PARTITION / 4).unwrap_or(0);
        if partition == 0 {
            return Err(ProtocolError::field_value(command, "partition", 0));
        }
        let row = sections.get(0);
        if row.len() != RESUME_ROW {
            return Err(ProtocolError::count_mismatch(
                command,
                "resume row",
                // The casts are lossless: a `usize` is at most 64 bits.
                row.len() as u64,
                RESUME_ROW as u64,
            ));
        }
        let terms = TermTableView::read(
            command,
            (1, 2),
            sections.offset(1),
            sections.get(1),
            sections.get(2),
        )?;
        let len = terms.len();
        let required = |at| position(row, at, len, false).map(|index| index.unwrap_or(ABSENT));
        let row = ResumeRow::new(
            position(row, 0, len, true)?,
            required(1)?,
            required(2)?,
            required(3)?,
            position(row, 4, len, true)?,
        );
        Ok(Self {
            pattern: bytes_at(fixed, 8).map_or(0, u64::from_le_bytes),
            scope: bytes_at(fixed, 16).map_or(0, u64::from_le_bytes),
            partition,
            ordering: sections.get(3),
            terms,
            row,
        })
    }

    /// The pattern's hash.
    #[must_use]
    pub fn pattern(&self) -> u64 {
        self.pattern
    }

    /// What binds the key to its transaction.
    #[must_use]
    pub fn scope(&self) -> u64 {
        self.scope
    }

    /// The partition that answered the last row, at least 1.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The last row's ordering value; empty for a family ordered by neighbour.
    #[must_use]
    pub fn ordering(&self) -> &'a [u8] {
        self.ordering
    }

    /// The key's term table.
    #[must_use]
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    /// The last row.
    #[must_use]
    pub fn row(&self) -> ResumeRow {
        self.row
    }
}
