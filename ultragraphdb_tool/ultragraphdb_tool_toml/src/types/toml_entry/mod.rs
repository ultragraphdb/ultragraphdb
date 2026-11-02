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
    taken: bool,
}

impl TomlEntry {
    /// An entry of `key` and `value`, not yet taken by a read.
    pub(crate) fn new(key: String, value: TomlValue) -> Self {
        Self {
            key,
            value,
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

    /// Whether a read took the entry.
    pub(crate) fn is_taken(&self) -> bool {
        self.taken
    }

    /// Marks the entry taken by a read.
    pub(crate) fn take(&mut self) {
        self.taken = true;
    }
}
