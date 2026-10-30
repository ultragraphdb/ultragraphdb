/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod result_view_body_view;

use crate::types::result_term_view::ResultTermView;
use crate::types::string_list::StringList;
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
    statement_ids: &'a [u8],
}

impl<'a> ResultView<'a> {
    pub fn columns(&self) -> u32 {
        self.columns
    }

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

    /// Whether the batch carries each row's statement ID.
    pub fn has_statement_ids(&self) -> bool {
        !self.statement_ids.is_empty()
    }

    /// The ID of the statement row `row` read; `None` past the last row or for a batch that carries none.
    pub fn statement_id(&self, row: u32) -> Option<u64> {
        let at = 8 * row as usize;
        self.statement_ids
            .get(at..at + 8)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u64::from_le_bytes)
    }

    pub fn resume(&self) -> &'a [u8] {
        self.resume
    }
}
