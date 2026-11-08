/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod result_view_body_view;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::monotonic::Monotonic;
use crate::types::result_term_view::ResultTermView;
use crate::types::string_list::StringList;
use crate::utils::constants::{ABSENT, PATTERN_COLUMNS_MAX};
use crate::utils::sections::u32_at;

/// The body of a received Result frame, read in place; see [`crate::ResultBatch`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultView<'a> {
    columns: u32,
    rows: u32,
    cells: &'a [u8],
    offsets: Vec<u32>,
    encodings: &'a [u8],
    texts: &'a [u8],
    strings: StringList<'a>,
    resume: &'a [u8],
    epoch: u64,
    partition: u32,
    monotonic: Monotonic,
    column_names: StringList<'a>,
}

impl<'a> ResultView<'a> {
    /// The number of columns.
    pub fn columns(&self) -> u32 {
        self.columns
    }

    /// The number of rows.
    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// The cell of `column` and `row`: an index into the term table, [`crate::ABSENT`] for unbound; `None` outside
    /// the batch.
    pub fn cell(&self, column: u32, row: u32) -> Option<u32> {
        if column >= self.columns || row >= self.rows {
            return None;
        }
        u32_at(
            self.cells,
            column as usize * self.rows as usize + row as usize,
        )
    }

    /// The number of terms in the table.
    pub fn term_count(&self) -> usize {
        self.offsets.len()
    }

    /// Term `index` of the table, `None` past the last.
    pub fn term(&self, index: u32) -> Option<ResultTermView<'a>> {
        let index = index as usize;
        let start = *self.offsets.get(index)? as usize;
        let end = self
            .offsets
            .get(index + 1)
            .map_or(self.encodings.len(), |end| *end as usize);
        let text = |at: usize| u32_at(self.texts, at).and_then(|i| self.strings.get(i as usize));
        Some(ResultTermView::new(
            self.encodings.get(start..end)?,
            text(2 * index),
            text(2 * index + 1),
        ))
    }

    /// The epoch the rows were read at; 0 where none applies.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The partition the read reached alone; 0 for several.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The resume key a limited read continues from; empty for none, and on every batch but the stream's last.
    pub fn resume(&self) -> &'a [u8] {
        self.resume
    }

    /// What the rows read as.
    #[must_use]
    pub fn monotonic(&self) -> Monotonic {
        self.monotonic
    }

    /// The column names: one per column on a query's first batch, empty on every other batch and on a pattern's.
    #[must_use]
    pub fn column_names(&self) -> StringList<'a> {
        self.column_names
    }

    /// The rules a receiver that asked a quad pattern holds its batch to (`PROTOCOL.md` §9.1), which a batch's bytes
    /// alone do not tell from a query's: `columns` 1 to [`PATTERN_COLUMNS_MAX`]; each cell of the reifier column, the
    /// last, [`ABSENT`] or a term of tag `0x02`.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::field_value`] `"columns"` for a column count outside the range; [`ProtocolError::field_value`]
    /// `"reifier"`, naming the row, for a reifier cell neither [`ABSENT`] nor of tag `0x02`.
    pub fn check_pattern(&self) -> Result<(), ProtocolError> {
        let command = Command::Result.number();
        if !(1..=PATTERN_COLUMNS_MAX).contains(&self.columns) {
            return Err(ProtocolError::field_value(
                command,
                "columns",
                self.columns.into(),
            ));
        }
        let reifier = self.columns.saturating_sub(1);
        for row in 0..self.rows {
            let cell = self.cell(reifier, row).unwrap_or(ABSENT);
            let blank = cell == ABSENT || self.term(cell).is_some_and(|t| t.is_blank_node());
            if !blank {
                return Err(ProtocolError::field_value(command, "reifier", row.into()));
            }
        }
        Ok(())
    }
}
