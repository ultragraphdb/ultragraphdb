/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod result_batch_body_builder;

use std::collections::HashMap;

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::result_term::ResultTerm;
use crate::utils::constants::ABSENT;
use crate::utils::sections::as_u32;

/// A columnar, dictionary-encoded result batch: per column a u32 cell per row indexing the batch's term table,
/// [`crate::ABSENT`] for unbound; the term table holding each distinct term of the batch once, in
/// `ultragraphdb_core_terms`' encoding, with a text section for what an encoding names but does not hold. The
/// header's flag [`crate::FLAG_LAST`] marks a stream's last batch, whose resume key, when not empty, continues a
/// limited read.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `columns` | u32 |
/// | 4 | `rows` | u32 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | cells, u32s, column by column: `columns × rows` |
/// | 1 | each term's offset in section 2, u32s, the first 0 |
/// | 2 | the terms' encodings, back to back |
/// | 3 | each term's text and annotation, two u32 string indices or [`crate::ABSENT`] |
/// | 4 | strings: ends |
/// | 5 | strings: text |
/// | 6 | the resume key |
/// | 7 | the statement IDs, u64s, one per row; empty unless the request asked for them |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultBatch {
    columns: u32,
    rows: u32,
    cells: Vec<u32>,
    terms: Vec<ResultTerm>,
    index: HashMap<Vec<u8>, u32>,
    resume: Vec<u8>,
    statement_ids: Option<Vec<u64>>,
}

impl ResultBatch {
    /// An empty batch of `columns` columns.
    pub fn new(columns: u32) -> Self {
        Self {
            columns,
            rows: 0,
            cells: Vec::new(),
            terms: Vec::new(),
            index: HashMap::new(),
            resume: Vec::new(),
            statement_ids: None,
        }
    }

    /// An empty batch of `columns` columns carrying each row's statement ID, for a request that asked for them.
    pub fn with_statement_ids(columns: u32) -> Self {
        Self {
            statement_ids: Some(Vec::new()),
            ..Self::new(columns)
        }
    }

    /// Appends a row, a term or `None` (unbound) per column; a term already in the table is not added again. Refused
    /// as [`ProtocolError::CountMismatch`] when the row's length is not the batch's column count, or when the batch
    /// carries statement IDs ([`ResultBatch::push_row_with_id`]).
    pub fn push_row(&mut self, row: &[Option<&ResultTerm>]) -> Result<(), ProtocolError> {
        if self.statement_ids.is_some() {
            return Err(Self::ids_mismatch(0, 1));
        }
        self.push_cells(row)
    }

    /// Appends a row as [`ResultBatch::push_row`] does, with the ID of the statement it read. Refused as
    /// [`ProtocolError::CountMismatch`] when the batch carries no statement IDs.
    pub fn push_row_with_id(
        &mut self,
        row: &[Option<&ResultTerm>],
        statement_id: u64,
    ) -> Result<(), ProtocolError> {
        if self.statement_ids.is_none() {
            return Err(Self::ids_mismatch(1, 0));
        }
        self.push_cells(row)?;
        if let Some(ids) = self.statement_ids.as_mut() {
            ids.push(statement_id);
        }
        Ok(())
    }

    /// A row pushed with `found` statement IDs to a batch taking `expected` per row.
    fn ids_mismatch(found: u64, expected: u64) -> ProtocolError {
        ProtocolError::CountMismatch(Command::Result.number(), "statement ids", found, expected)
    }

    fn push_cells(&mut self, row: &[Option<&ResultTerm>]) -> Result<(), ProtocolError> {
        if row.len() != self.columns as usize {
            return Err(ProtocolError::CountMismatch(
                Command::Result.number(),
                "row",
                row.len() as u64,
                self.columns.into(),
            ));
        }
        for cell in row {
            let index = match cell {
                None => ABSENT,
                Some(term) => {
                    let encoding = term.encoding();
                    match self.index.get(&encoding) {
                        Some(index) => *index,
                        None => {
                            let index = as_u32(self.terms.len());
                            self.index.insert(encoding, index);
                            self.terms.push((*term).clone());
                            index
                        }
                    }
                }
            };
            self.cells.push(index);
        }
        self.rows = self.rows.saturating_add(1);
        Ok(())
    }

    /// The batch with the resume key a limited read continues from.
    pub fn with_resume(mut self, resume: Vec<u8>) -> Self {
        self.resume = resume;
        self
    }

    pub fn columns(&self) -> u32 {
        self.columns
    }

    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// The term table, in order of first appearance.
    pub fn terms(&self) -> &[ResultTerm] {
        &self.terms
    }

    /// The cell of `column` and `row`: an index into the term table, [`crate::ABSENT`] for unbound; `None` outside
    /// the batch.
    pub fn cell(&self, column: u32, row: u32) -> Option<u32> {
        if column >= self.columns || row >= self.rows {
            return None;
        }
        self.cells
            .get(row as usize * self.columns as usize + column as usize)
            .copied()
    }

    pub fn resume(&self) -> &[u8] {
        &self.resume
    }

    /// Each row's statement ID, in row order; `None` for a batch that carries none.
    pub fn statement_ids(&self) -> Option<&[u64]> {
        self.statement_ids.as_deref()
    }

    /// The bytes the batch's body takes, before its chunk table, so a sender cuts batches below the maximum frame.
    pub fn encoded_len(&self) -> usize {
        self.body().len()
    }
}
