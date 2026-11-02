/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::toml_entry::TomlEntry;

/// A table of the document: its name and its entries in the order the document lists them. The root table, which
/// holds the keys before any header, has the empty name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TomlTable {
    name: String,
    entries: Vec<TomlEntry>,
}

impl TomlTable {
    /// An empty table named `name`, empty for the root table.
    pub(crate) fn new(name: String) -> Self {
        Self {
            name,
            entries: Vec::new(),
        }
    }

    /// The table's name, as its header gives it; empty for the root table.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The table's entries, in the document's order.
    #[must_use]
    pub fn entries(&self) -> &[TomlEntry] {
        &self.entries
    }

    /// Appends `entry`, whose key the parser checked is not in the table yet.
    pub(crate) fn push(&mut self, entry: TomlEntry) {
        debug_assert!(self.entries.iter().all(|e| e.key() != entry.key()));
        self.entries.push(entry);
    }

    /// The entry of `key`, if the table holds it.
    pub(crate) fn entry_mut(&mut self, key: &str) -> Option<&mut TomlEntry> {
        self.entries.iter_mut().find(|e| e.key() == key)
    }
}
