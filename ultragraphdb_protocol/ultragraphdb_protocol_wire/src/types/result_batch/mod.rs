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
/// limited read (`PROTOCOL.md` §9.1). The columns are the pattern's unbound slots, in slot order, then the reifier
/// column: a row names its statement by its quad and, when the statement holds a statement ID, by its reifier's
/// skolem IRI, a blank-node term; no row carries a statement ID.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `columns` | u32: the unbound slots and the reifier column, 1 to 5 |
/// | 4 | `rows` | u32 |
/// | 8 | `epoch` | u64: the epoch the rows were read at; 0 where none applies |
/// | 16 | `partition` | u32: the partition the read reached alone; 0 for several |
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultBatch {
    columns: u32,
    rows: u32,
    cells: Vec<u32>,
    terms: Vec<ResultTerm>,
    index: HashMap<Vec<u8>, u32>,
    resume: Vec<u8>,
    epoch: u64,
    partition: u32,
}

impl ResultBatch {
    /// An empty batch of `columns` columns, the last the reifier column.
    pub fn new(columns: u32) -> Self {
        Self {
            columns,
            rows: 0,
            cells: Vec::new(),
            terms: Vec::new(),
            index: HashMap::new(),
            resume: Vec::new(),
            epoch: 0,
            partition: 0,
        }
    }

    /// Appends a row, a term or `None` (unbound) per column, the last its statement's reifier; a term already in the
    /// table is not added again. Refused as [`ProtocolError::count_mismatch`] (`"row"`) when the row's length is not
    /// the batch's column count, and as [`ProtocolError::field_value`] (`"reifier"`) when its reifier is not a blank
    /// node.
    pub fn push_row(&mut self, row: &[Option<&ResultTerm>]) -> Result<(), ProtocolError> {
        // A row of the wrong width is refused by its width first, as `push_cells` checks it.
        if row.len() == self.columns as usize
            && let Some(Some(reifier)) = row.last()
            && !reifier.is_blank_node()
        {
            return Err(ProtocolError::field_value(
                Command::Result.number(),
                "reifier",
                row.len() as u64 - 1,
            ));
        }
        self.push_cells(row)
    }

    /// The batch read at `epoch` in `partition`, 0 for none and for several.
    pub fn with_read_at(mut self, epoch: u64, partition: u32) -> Self {
        self.epoch = epoch;
        self.partition = partition;
        self
    }

    /// The epoch the rows were read at; 0 where none applies.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The partition the read reached alone; 0 for several.
    pub fn partition(&self) -> u32 {
        self.partition
    }

    fn push_cells(&mut self, row: &[Option<&ResultTerm>]) -> Result<(), ProtocolError> {
        if row.len() != self.columns as usize {
            return Err(ProtocolError::count_mismatch(
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
                            let _absent: Option<u32> = self.index.insert(encoding, index);
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

    /// The number of columns.
    pub fn columns(&self) -> u32 {
        self.columns
    }

    /// The number of rows.
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

    /// The resume key a limited read continues from; empty for none.
    pub fn resume(&self) -> &[u8] {
        &self.resume
    }

    /// The bytes the batch's body takes, before its chunk table, so a sender cuts batches below the maximum frame.
    pub fn encoded_len(&self) -> usize {
        self.body().len()
    }
}
