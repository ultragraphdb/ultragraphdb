/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod write_body_builder;

use ultragraphdb_core_mapping::Changeset;

/// A changeset (in a transaction on the frame's stream): the mapping crate's [`Changeset`] in this layout, its terms
/// N-Triples strings, the IRIs its declarations name plain strings.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `namespace` | u64 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | one change record of 16 bytes per change, in order: `kind` u8, `flags` u8, 2 zero bytes, `args_start` u32, `args_len` u32, 4 zero bytes |
/// | 1 | args, u32s: string indices, [`crate::ABSENT`], counts and halves of statement IDs, as each kind defines |
/// | 2 | strings: ends |
/// | 3 | strings: text |
///
/// The kinds and their args are tabled in the crate's change records (`utils/changes.rs`): Insert 1, InsertEdge 2,
/// Delete 3, DeleteStatement 4, DeletePattern 5, SetProperty 6, DeleteProperty 7, and declarations of a predicate 8,
/// an entity type 9 and a relation 10.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    changeset: Changeset,
}

impl Write {
    pub fn new(changeset: Changeset) -> Self {
        Self { changeset }
    }

    pub fn changeset(&self) -> &Changeset {
        &self.changeset
    }
}
