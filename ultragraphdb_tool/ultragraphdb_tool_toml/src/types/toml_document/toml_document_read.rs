/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::toml_error::TomlError;
use crate::types::toml_document::TomlDocument;
use crate::types::toml_kind::TomlKind;
use crate::types::toml_value::TomlValue;

impl TomlDocument {
    /// Takes the string setting `key` of table `table`, the empty name for the root table.
    ///
    /// # Errors
    ///
    /// `Missing` when the document holds no such setting, `WrongKind` when it holds another kind of value.
    pub fn string(&mut self, table: &str, key: &str) -> Result<String, TomlError> {
        self.take(table, key, TomlKind::String, |value| match value {
            TomlValue::String(text) => Some(text.clone()),
            TomlValue::Integer(_) | TomlValue::Boolean(_) => None,
        })
    }

    /// Takes the integer setting `key` of table `table`, the empty name for the root table.
    ///
    /// # Errors
    ///
    /// `Missing` when the document holds no such setting, `WrongKind` when it holds another kind of value.
    pub fn integer(&mut self, table: &str, key: &str) -> Result<i64, TomlError> {
        self.take(table, key, TomlKind::Integer, |value| match value {
            TomlValue::Integer(number) => Some(*number),
            TomlValue::String(_) | TomlValue::Boolean(_) => None,
        })
    }

    /// Takes the boolean setting `key` of table `table`, the empty name for the root table.
    ///
    /// # Errors
    ///
    /// `Missing` when the document holds no such setting, `WrongKind` when it holds another kind of value.
    pub fn boolean(&mut self, table: &str, key: &str) -> Result<bool, TomlError> {
        self.take(table, key, TomlKind::Boolean, |value| match value {
            TomlValue::Boolean(flag) => Some(*flag),
            TomlValue::String(_) | TomlValue::Integer(_) => None,
        })
    }

    /// Ends the reading of the settings: every setting must have been taken.
    ///
    /// # Errors
    ///
    /// `UnreadKey` naming the first setting, in the document's order, that no read took.
    pub fn finish(self) -> Result<(), TomlError> {
        let unread = self.tables.iter().find_map(|table| {
            table
                .entries()
                .iter()
                .find(|entry| !entry.is_taken())
                .map(|entry| TomlError::unread_key(table.name(), entry.key()))
        });
        unread.map_or(Ok(()), Err)
    }

    /// The value of `key` in `table` as `pick` reads it for the kind `expected`, marking the setting taken; a
    /// value `pick` does not read is of another kind, and is left untaken.
    fn take<T>(
        &mut self,
        table: &str,
        key: &str,
        expected: TomlKind,
        pick: impl Fn(&TomlValue) -> Option<T>,
    ) -> Result<T, TomlError> {
        let entry = self
            .tables
            .iter_mut()
            .find(|t| t.name() == table)
            .and_then(|t| t.entry_mut(key))
            .ok_or_else(|| TomlError::missing(table, key, expected))?;
        let found = entry.value().kind();
        let value = pick(entry.value())
            .ok_or_else(|| TomlError::wrong_kind(table, key, expected, found))?;
        debug_assert_eq!(found, expected);
        entry.take();
        Ok(value)
    }
}
