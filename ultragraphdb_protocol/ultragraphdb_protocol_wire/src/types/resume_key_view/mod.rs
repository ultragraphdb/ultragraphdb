/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use xxhash_rust::xxh3::xxh3_64;

use crate::types::command::Command;
use crate::types::resume_key::RESUME_FIXED;
use crate::types::resume_row::ResumeRow;
use crate::types::term_table_view::TermTableView;
use crate::utils::constants::ABSENT;
use crate::utils::le::bytes_at;
use crate::utils::sections::{Sections, as_u32, u32_at};

/// The bytes of a resume key's last row: five u32 indices.
const RESUME_ROW: usize = 20;

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
    terms: TermTableView<'a>,
    row: ResumeRow,
}

impl<'a> ResumeKeyView<'a> {
    /// The key `bytes`, checked once.
    ///
    /// # Errors
    ///
    /// In order: its checksum ([`ProtocolError::field_value`] `"resume key"`); the body layout; section 0 of 20 bytes
    /// ([`ProtocolError::count_mismatch`] `"resume row"`); the term table; each index below the table's length,
    /// `ABSENT` only for the graph and the reifier ([`ProtocolError::index_out_of_range`]).
    pub fn read(bytes: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::ComputeRead.number();
        let checksum = bytes_at(bytes, 0).map(u64::from_le_bytes);
        if checksum != Some(xxh3_64(bytes.get(8..).unwrap_or_default())) {
            return Err(ProtocolError::field_value(command, "resume key", 0));
        }
        let sections = Sections::<3>::read(command, bytes, RESUME_FIXED)?;
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
        let fixed = sections.fixed();
        Ok(Self {
            pattern: bytes_at(fixed, 8).map_or(0, u64::from_le_bytes),
            scope: bytes_at(fixed, 16).map_or(0, u64::from_le_bytes),
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
