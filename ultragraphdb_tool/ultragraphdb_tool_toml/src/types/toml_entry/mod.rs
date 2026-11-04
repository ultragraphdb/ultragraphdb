/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::toml_value::TomlValue;

/// A key of a table and its value, as the document holds them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TomlEntry {
    key: String,
    value: TomlValue,
    line: usize,
    taken: bool,
}

impl TomlEntry {
    /// An entry of `key` and `value` on line `line`, not yet taken by a read.
    pub(crate) fn new(key: String, value: TomlValue, line: usize) -> Self {
        Self {
            key,
            value,
            line,
            taken: false,
        }
    }

    /// The bare key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The value.
    #[must_use]
    pub fn value(&self) -> &TomlValue {
        &self.value
    }

    /// The line it stands on, counting from 1.
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// Whether a read took the entry.
    pub(crate) fn is_taken(&self) -> bool {
        self.taken
    }

    /// Marks the entry taken by a read.
    pub(crate) fn take(&mut self) {
        self.taken = true;
    }
}
