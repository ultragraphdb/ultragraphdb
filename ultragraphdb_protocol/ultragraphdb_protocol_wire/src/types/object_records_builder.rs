/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::as_u32;

/// A value of a keyspace of lists being written: object records back to back, in the list's order; see
/// [`crate::ObjectRecords`] for their layout.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjectRecordsBuilder {
    value: Vec<u8>,
}

impl ObjectRecordsBuilder {
    /// A value of no records.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends the record of the object `encoding`, held under `statements`.
    pub fn push(&mut self, encoding: &[u8], statements: &[[u8; 16]]) {
        self.value
            .extend_from_slice(&as_u32(encoding.len()).to_le_bytes());
        self.value
            .extend_from_slice(&as_u32(statements.len()).to_le_bytes());
        self.value.extend_from_slice(encoding);
        statements
            .iter()
            .for_each(|id| self.value.extend_from_slice(id));
    }

    /// The records so far, the value [`crate::Entries::push`] takes.
    #[must_use]
    pub fn value(&self) -> &[u8] {
        &self.value
    }
}
