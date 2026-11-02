/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod toml_document_read;

use crate::errors::toml_error::TomlError;

use crate::types::toml_parser::TomlParser;
use crate::types::toml_table::TomlTable;

/// A settings file, parsed: the root table first, then each table in the order of its header.
///
/// A role reads its settings through [`string`](Self::string), [`integer`](Self::integer) and
/// [`boolean`](Self::boolean), each taking the setting it reads, then calls [`finish`](Self::finish), which refuses
/// any setting left untaken.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TomlDocument {
    tables: Vec<TomlTable>,
}

impl TomlDocument {
    /// The most bytes a document holds: 64 KiB.
    pub const BYTES_MAX: usize = 65_536;
    /// The most bytes a line holds, without its line ending: 4 KiB.
    pub const LINE_BYTES_MAX: usize = 4_096;
    /// The most tables with a header a document holds; the root table is not counted.
    pub const TABLES_MAX: usize = 256;
    /// The most keys a document holds, in every table together.
    pub const KEYS_MAX: usize = 4_096;

    /// Parses `bytes` as a document of the subset, in one pass with no recursion.
    ///
    /// # Errors
    ///
    /// The [`TomlError`] of the first refusal in the document's order: `Limit` for a document past
    /// [`BYTES_MAX`](Self::BYTES_MAX), a line past [`LINE_BYTES_MAX`](Self::LINE_BYTES_MAX), a table past
    /// [`TABLES_MAX`](Self::TABLES_MAX) or a key past [`KEYS_MAX`](Self::KEYS_MAX); `Unsupported` naming a feature
    /// outside the subset and its line; `Invalid` naming the rule of TOML 1.0 a line breaks.
    pub fn parse(bytes: &[u8]) -> Result<Self, TomlError> {
        let tables = TomlParser::parse(bytes)?;
        debug_assert!(tables.first().is_some_and(|root| root.name().is_empty()));
        debug_assert!(tables.len() <= Self::TABLES_MAX + 1);
        Ok(Self { tables })
    }

    /// The tables, the root table first.
    #[must_use]
    pub fn tables(&self) -> &[TomlTable] {
        &self.tables
    }
}
