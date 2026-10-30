/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod write_body_builder;

use crate::types::term_table::TermTable;
use crate::types::write_change::WriteChange;

/// A changeset in the transaction of the frame's stream, in the transaction's namespace, in the wire's own layout: its
/// changes in order, their terms indices into the Write's term table, the IRIs, tokenizers and options its
/// declarations name in a list of names. The server session converts it to the mapping crate's `Changeset`. The
/// command header is all zero.
///
/// | Section | Contents |
/// |---|---|
/// | 0 | one change record of 16 bytes per change, in order: `kind` u8, `flags` u8, 2 zero bytes, `args_start` u32, `args_len` u32, 4 zero bytes |
/// | 1 | args, u32s: term indices, name indices, [`crate::ABSENT`], counts and halves of statement IDs, as each kind defines |
/// | 2 | the term table's records |
/// | 3 | the term table's text |
/// | 4 | names: ends |
/// | 5 | names: text |
///
/// The kinds and their args are tabled in the crate's change records (`utils/changes.rs`): Insert 1, InsertEdge 2,
/// Delete 3, DeleteStatement 4, DeletePattern 5, SetProperty 6, DeleteProperty 7, and declarations of a predicate 8,
/// an entity type 9 and a relation 10.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    terms: TermTable,
    changes: Vec<WriteChange>,
}

impl Write {
    pub fn new(terms: TermTable, changes: Vec<WriteChange>) -> Self {
        Self { terms, changes }
    }

    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    pub fn changes(&self) -> &[WriteChange] {
        &self.changes
    }
}
