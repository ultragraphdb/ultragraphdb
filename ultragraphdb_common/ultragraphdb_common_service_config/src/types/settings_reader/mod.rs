/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_tool_toml::{TomlDocument, TomlError, TomlErrorEnum};

use crate::errors::service_config_error::ServiceConfigError;

/// What a variable overriding a setting begins with.
const VARIABLE_PREFIX: &str = "ULTRAGRAPHDB_";

/// The tables a settings file may hold, each mirrored by one type.
const TABLES: [&str; 9] = [
    "cluster",
    "compute",
    "endpoint",
    "frontend",
    "metadata",
    "observability",
    "runtime",
    "sequencer",
    "storage",
];

/// One start's reading of its settings: each setting taken from the file, then overridden by its variable, so that
/// [`SettingsReader::finish`] can refuse whatever no read took.
pub(crate) struct SettingsReader {
    document: Option<TomlDocument>,
    variables: BTreeMap<String, String>,
    taken: BTreeSet<String>,
}

impl SettingsReader {
    /// The reading of the file holding `bytes`, if one is named, and the environment `variables`. The variables that
    /// do not begin with the prefix belong to someone else and are dropped.
    pub(crate) fn new(
        bytes: Option<&[u8]>,
        variables: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, ServiceConfigError> {
        let document = bytes.map(TomlDocument::parse).transpose()?;
        if let Some(unknown) = document
            .iter()
            .flat_map(TomlDocument::tables)
            .find(|table| !table.name().is_empty() && !TABLES.contains(&table.name()))
        {
            return Err(ServiceConfigError::unknown_table(unknown.name()));
        }
        Ok(Self {
            document,
            variables: variables
                .into_iter()
                .filter(|(name, _)| name.starts_with(VARIABLE_PREFIX))
                .collect(),
            taken: BTreeSet::new(),
        })
    }

    /// The string setting `key` of `table`; `None` when neither the file nor the environment names it.
    pub(crate) fn string(
        &mut self,
        table: &str,
        key: &str,
    ) -> Result<Option<String>, ServiceConfigError> {
        let in_file = self.in_file(|document| document.string(table, key))?;
        Ok(self
            .take(table, key)
            .map_or(in_file, |(_, value)| Some(value)))
    }

    /// The boolean setting `key` of `table`.
    pub(crate) fn boolean(
        &mut self,
        table: &str,
        key: &str,
    ) -> Result<Option<bool>, ServiceConfigError> {
        let in_file = self.in_file(|document| document.boolean(table, key))?;
        match self.take(table, key) {
            None => Ok(in_file),
            Some((_, value)) if value == "true" => Ok(Some(true)),
            Some((_, value)) if value == "false" => Ok(Some(false)),
            Some((variable, value)) => Err(ServiceConfigError::wrong_kind_variable(
                variable,
                value,
                "a boolean",
            )),
        }
    }

    /// The integer setting `key` of `table`, as a `u32`.
    pub(crate) fn u32(
        &mut self,
        table: &str,
        key: &str,
    ) -> Result<Option<u32>, ServiceConfigError> {
        self.integer(table, key)?
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    ServiceConfigError::out_of_range(table, key, value, u64::from(u32::MAX))
                })
            })
            .transpose()
    }

    /// The integer setting `key` of `table`, as a `u64`.
    pub(crate) fn u64(
        &mut self,
        table: &str,
        key: &str,
    ) -> Result<Option<u64>, ServiceConfigError> {
        self.integer(table, key)?
            .map(|value| {
                u64::try_from(value).map_err(|_| {
                    ServiceConfigError::out_of_range(table, key, value, i64::MAX.unsigned_abs())
                })
            })
            .transpose()
    }

    /// Ends the reading: a setting of the file or a variable no read took is refused.
    pub(crate) fn finish(self) -> Result<(), ServiceConfigError> {
        if let Some(document) = self.document {
            document.finish()?;
        }
        match self
            .variables
            .keys()
            .find(|name| !self.taken.contains(*name))
        {
            Some(unknown) => Err(ServiceConfigError::unknown_variable(unknown)),
            None => Ok(()),
        }
    }

    fn integer(&mut self, table: &str, key: &str) -> Result<Option<i64>, ServiceConfigError> {
        let in_file = self.in_file(|document| document.integer(table, key))?;
        match self.take(table, key) {
            None => Ok(in_file),
            Some((variable, value)) => match value.parse() {
                Ok(number) => Ok(Some(number)),
                Err(_) => Err(ServiceConfigError::wrong_kind_variable(
                    variable,
                    value,
                    "an integer",
                )),
            },
        }
    }

    /// What `read` takes from the file, a setting it does not hold being absent. Taking it even when a variable
    /// overrides it checks its kind, and marks it read.
    fn in_file<T>(
        &mut self,
        read: impl FnOnce(&mut TomlDocument) -> Result<T, TomlError>,
    ) -> Result<Option<T>, ServiceConfigError> {
        let Some(document) = self.document.as_mut() else {
            return Ok(None);
        };
        match read(document) {
            Ok(value) => Ok(Some(value)),
            Err(TomlError(TomlErrorEnum::Missing { .. })) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// The variable overriding `table.key`, with its value, recorded as taken whether set or not.
    fn take(&mut self, table: &str, key: &str) -> Option<(String, String)> {
        let name = format!(
            "{VARIABLE_PREFIX}{}_{}",
            table.to_ascii_uppercase(),
            key.to_ascii_uppercase()
        );
        let value = self.variables.get(&name).cloned();
        let _new: bool = self.taken.insert(name.clone());
        value.map(|value| (name, value))
    }
}
