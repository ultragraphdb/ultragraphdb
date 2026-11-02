/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod toml_parser_line;

use std::collections::BTreeSet;

use crate::errors::toml_error::TomlError;
use crate::types::toml_document::TomlDocument;
use crate::types::toml_invalid::TomlInvalid;
use crate::types::toml_limit::TomlLimit;
use crate::types::toml_table::TomlTable;

/// The single pass over a document: line by line, each line once from its start to its end, with no recursion and
/// no look back but the sets of names already defined.
pub(crate) struct TomlParser<'a> {
    /// The tables before the current one, the root table first.
    done: Vec<TomlTable>,
    /// The table the lines are filling: the root table until the first header.
    current: TomlTable,
    /// The names of the tables with a header.
    names: BTreeSet<&'a str>,
    /// The keys of the root table, which a header may not name.
    root_keys: BTreeSet<&'a str>,
    /// The keys of the current table.
    keys: BTreeSet<&'a str>,
    /// The keys of every table together.
    key_count: usize,
}

impl<'a> TomlParser<'a> {
    /// The tables of the document `bytes`, the root table first.
    ///
    /// # Errors
    ///
    /// As [`TomlDocument::parse`].
    pub(crate) fn parse(bytes: &'a [u8]) -> Result<Vec<TomlTable>, TomlError> {
        if bytes.len() > TomlDocument::BYTES_MAX {
            return Err(TomlError::limit(
                TomlLimit::DocumentBytes,
                TomlDocument::BYTES_MAX,
            ));
        }
        let mut parser = Self {
            done: Vec::new(),
            current: TomlTable::new(String::new()),
            names: BTreeSet::new(),
            root_keys: BTreeSet::new(),
            keys: BTreeSet::new(),
            key_count: 0,
        };
        // A line feed never occurs inside a UTF-8 sequence, so each line is checked as UTF-8 on its own. A carriage
        // return ends a line only before its line feed; anywhere else, the last line included, it is a control
        // character.
        for (index, raw) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
            let line = index.saturating_add(1);
            let raw = match raw.strip_suffix(b"\n") {
                Some(content) => content.strip_suffix(b"\r").unwrap_or(content),
                None => raw,
            };
            if raw.len() > TomlDocument::LINE_BYTES_MAX {
                return Err(TomlError::limit(
                    TomlLimit::LineBytes,
                    TomlDocument::LINE_BYTES_MAX,
                ));
            }
            let text = std::str::from_utf8(raw)
                .map_err(|_| TomlError::invalid(TomlInvalid::InvalidUtf8, line))?;
            parser.line(text, line)?;
        }
        parser.done.push(parser.current);
        Ok(parser.done)
    }

    /// Starts the table `name`, whose header is on line `line`.
    fn define_table(&mut self, name: &'a str, line: usize) -> Result<(), TomlError> {
        debug_assert!(!name.is_empty());
        if self.done.len() >= TomlDocument::TABLES_MAX {
            return Err(TomlError::limit(
                TomlLimit::Tables,
                TomlDocument::TABLES_MAX,
            ));
        }
        if self.done.is_empty() {
            self.root_keys = std::mem::take(&mut self.keys);
        } else {
            self.keys.clear();
        }
        if self.root_keys.contains(name) {
            return Err(TomlError::invalid(
                TomlInvalid::DuplicateKey(name.to_string()),
                line,
            ));
        }
        if !self.names.insert(name) {
            return Err(TomlError::invalid(
                TomlInvalid::DuplicateTable(name.to_string()),
                line,
            ));
        }
        let finished = std::mem::replace(&mut self.current, TomlTable::new(name.to_string()));
        self.done.push(finished);
        Ok(())
    }

    /// Claims `key`, on line `line`, for the current table, before its value is read.
    fn claim_key(&mut self, key: &'a str, line: usize) -> Result<(), TomlError> {
        debug_assert!(!key.is_empty());
        if self.key_count >= TomlDocument::KEYS_MAX {
            return Err(TomlError::limit(TomlLimit::Keys, TomlDocument::KEYS_MAX));
        }
        if !self.keys.insert(key) {
            return Err(TomlError::invalid(
                TomlInvalid::DuplicateKey(key.to_string()),
                line,
            ));
        }
        self.key_count = self.key_count.saturating_add(1);
        Ok(())
    }
}
