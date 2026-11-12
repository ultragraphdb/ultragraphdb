/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod result_batch_body_builder;

use std::collections::HashMap;

use ultragraphdb_core_body::as_u32;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::chunk_layout::ChunkLayout;
use crate::types::command::Command;
use crate::types::monotonic::Monotonic;
use crate::types::result_term::ResultTerm;
use crate::types::warning::Warning;
use crate::utils::constants::{ABSENT, HEADER_SIZE, WARNING_SIZE};

/// The section table of a batch's twelve sections and the padding of up to 7 bytes before each of the nine that can
/// follow a section ending off a multiple of 8: every one but section 0, which starts at the table's end, and sections 4
/// and 10, which follow the 8-byte texts and the 24-byte warnings.
const SECTIONS_OVERHEAD: usize = 96 + 9 * 7;

/// What a term takes beside its bytes: its offset, its text's and annotation's string indices, and their strings' two ends.
const TERM_OVERHEAD: usize = 4 + 8 + 8;

/// The bytes `term` takes in a batch beside [`TERM_OVERHEAD`]: its encoding, its text and its annotation.
fn term_size(term: &ResultTerm) -> usize {
    term.encoding()
        .len()
        .saturating_add(term.text().map_or(0, str::len))
        .saturating_add(term.annotation().map_or(0, str::len))
}

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
/// | 0 | `columns` | u32: for a quad pattern, the unbound slots and the reifier column, 1 to 5; for a query, its projections, 1 to 1,024, or 0 with `rows` 0 for a GQL statement without `RETURN` |
/// | 4 | `rows` | u32 |
/// | 8 | `epoch` | u64: the epoch the rows were read at; 0 where none applies |
/// | 16 | `partition` | u32: the partition the read reached alone; 0 for several |
/// | 20 | `monotonic` | u8, a [`Monotonic`] |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | cells, u32s, column by column: `columns × rows` |
/// | 1 | each term's offset in section 2, u32s, the first 0 |
/// | 2 | the terms' encodings, back to back |
/// | 3 | each term's text and annotation, two u32 string indices or [`crate::ABSENT`] |
/// | 4 | strings: ends |
/// | 5 | strings: text |
/// | 6 | the resume key; empty on every batch but the stream's last |
/// | 7 | column names: ends; one name per column on a query's first batch, empty otherwise |
/// | 8 | column names: text |
/// | 9 | the warnings (`PROTOCOL.md` §4.9): on a query's first batch, those its compilation reported or its Prepare answered; empty otherwise |
/// | 10 | the warnings' messages: ends |
/// | 11 | the warnings' messages: text |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultBatch {
    columns: u32,
    rows: u32,
    cells: Vec<u32>,
    terms: Vec<ResultTerm>,
    index: HashMap<Vec<u8>, u32>,
    term_bytes: usize,
    resume: Vec<u8>,
    epoch: u64,
    partition: u32,
    monotonic: Monotonic,
    column_names: Vec<String>,
    warnings: Vec<Warning>,
}

impl ResultBatch {
    /// An empty batch of `columns` columns, for a pattern the last the reifier column.
    pub fn new(columns: u32) -> Self {
        Self {
            columns,
            rows: 0,
            cells: Vec::new(),
            terms: Vec::new(),
            index: HashMap::new(),
            term_bytes: 0,
            resume: Vec::new(),
            epoch: 0,
            partition: 0,
            monotonic: Monotonic::Snapshot,
            column_names: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Appends a row, a term or `None` (unbound) per column, the last its statement's reifier; a term already in the
    /// table is not added again. Refused as [`ProtocolError::count_mismatch`] (`"row"`) when the row's length is not
    /// the batch's column count, and as [`ProtocolError::field_value`] (`"reifier"`) when its reifier is a literal: the
    /// reifier column holds a blank node, by its skolem IRI, or an IRI, a declared relation's handle IRI (`PROTOCOL.md`
    /// §9.1, §10.2).
    pub fn push_row(&mut self, row: &[Option<&ResultTerm>]) -> Result<(), ProtocolError> {
        // A row of the wrong width is refused by its width first, as `push_cells` checks it.
        if row.len() == self.columns as usize // lossless: a `usize` is at least 32 bits
            && let Some(Some(reifier)) = row.last()
            // An IRI or a blank node is named by its hash, a literal never.
            && reifier.hash().is_none()
        {
            return Err(ProtocolError::field_value(
                Command::Result.number(),
                "reifier",
                // Lossless: a `usize` is at most 64 bits; the row holds the reifier, so it never saturates.
                (row.len() as u64).saturating_sub(1),
            ));
        }
        self.push_cells(row)
    }

    /// Appends a row of a query's batch, a term or `None` (null) per column.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::count_mismatch`] (`"row"`) when the row's length is not the batch's column count.
    pub fn push_query_row(&mut self, row: &[Option<&ResultTerm>]) -> Result<(), ProtocolError> {
        self.push_cells(row)
    }

    /// The batch whose rows read as `monotonic` says.
    #[must_use]
    pub fn with_monotonic(mut self, monotonic: Monotonic) -> Self {
        self.monotonic = monotonic;
        self
    }

    /// The batch naming its columns, a query's first.
    #[must_use]
    pub fn with_column_names(mut self, column_names: Vec<String>) -> Self {
        self.column_names = column_names;
        self
    }

    /// What the rows read as.
    #[must_use]
    pub fn monotonic(&self) -> Monotonic {
        self.monotonic
    }

    /// The column names; empty on every batch but a query's first.
    #[must_use]
    pub fn column_names(&self) -> &[String] {
        &self.column_names
    }

    /// The batch read at `epoch` in `partition`, 0 for none and for several.
    #[must_use]
    pub fn with_read_at(mut self, epoch: u64, partition: u32) -> Self {
        self.epoch = epoch;
        self.partition = partition;
        self
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
                            self.term_bytes = self.term_bytes.saturating_add(term_size(term));
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
        // The casts are lossless: a `usize` is at least 32 bits.
        let at = (row as usize)
            .checked_mul(self.columns as usize)?
            .checked_add(column as usize)?;
        self.cells.get(at).copied()
    }

    /// The resume key a limited read continues from; empty for none.
    pub fn resume(&self) -> &[u8] {
        &self.resume
    }

    /// At most the bytes of the frame this batch would be, holding `row` too and the resume key of `resume` bytes, computed with
    /// no body built: a string two terms share counts twice, so a sender cuts a batch within its `max_frame` as it fills.
    #[must_use]
    pub fn frame_size_with(&self, row: &[Option<&ResultTerm>], resume: usize) -> usize {
        let mut counted: Vec<Vec<u8>> = Vec::new();
        let mut terms = self.terms.len();
        let mut bytes = self.term_bytes;
        for term in row.iter().copied().flatten() {
            let encoding = term.encoding();
            if !self.index.contains_key(&encoding) && !counted.contains(&encoding) {
                terms = terms.saturating_add(1);
                bytes = bytes.saturating_add(term_size(term));
                counted.push(encoding);
            }
        }
        // The casts are lossless: a `usize` is at least 32 bits.
        let rows = (self.rows as usize).saturating_add(usize::from(!row.is_empty()));
        let cells = rows.saturating_mul(self.columns as usize);
        // Each column name's end and bytes (sections 7 and 8).
        let names = self.column_names.iter().fold(0usize, |sum, name| {
            sum.saturating_add(4).saturating_add(name.len())
        });
        // Each warning's 24 bytes and its message's end and bytes, a message two warnings share counted twice.
        let warnings = self.warnings.iter().fold(0usize, |sum, warning| {
            sum.saturating_add(WARNING_SIZE + 4)
                .saturating_add(warning.message().len())
        });
        let data = SECTIONS_OVERHEAD
            .saturating_add(cells.saturating_mul(4))
            .saturating_add(terms.saturating_mul(TERM_OVERHEAD))
            .saturating_add(bytes)
            .saturating_add(resume)
            .saturating_add(names)
            .saturating_add(warnings);
        HEADER_SIZE.saturating_add(ChunkLayout::of_data(data).body_len())
    }

    /// The bytes the batch's body takes, before its chunk table, so a sender cuts batches below the maximum frame.
    pub fn encoded_len(&self) -> usize {
        self.body().len()
    }

    /// The batch with the warnings its query's compilation reported, or its Prepare answered: a query's first batch.
    #[must_use]
    pub fn with_warnings(self, warnings: Vec<Warning>) -> Self {
        Self { warnings, ..self }
    }

    /// The warnings; empty on every batch but a query's first.
    #[must_use]
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }
}
