/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod service_config_error_display;
mod service_config_error_error;
mod service_config_error_from;

use ultragraphdb_tool_toml::TomlError;

/// Why a configuration was refused. A role exits with code 78 on any of them. The classification is the public field
/// `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceConfigErrorEnum {
    /// The file's TOML was refused, or a setting in it is unread or of the wrong kind.
    Toml(TomlError),
    /// The file holds a table no type mirrors.
    UnknownTable {
        /// The table, as the file names it.
        table: String,
    },
    /// A value does not fit its setting's field.
    OutOfRange {
        /// The setting, `table.key`.
        setting: String,
        /// The value found.
        value: i64,
        /// The largest value the field holds.
        max: u64,
    },
    /// A setting a role requires is absent.
    Missing {
        /// The setting, `table.key`.
        setting: String,
    },
    /// The endpoint names neither a certificate and key nor `plaintext = true`.
    NoEndpointSecurity,
    /// The endpoint names a certificate or key together with `plaintext = true`.
    ConflictingEndpointSecurity,
    /// The endpoint names only one of its certificate and key.
    IncompleteEndpointSecurity {
        /// The setting that is absent.
        absent: &'static str,
    },
}

/// A refused configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfigError(pub ServiceConfigErrorEnum);

impl ServiceConfigError {
    /// The refusal of the table `table`, which no type mirrors.
    pub fn unknown_table(table: impl Into<String>) -> Self {
        Self(ServiceConfigErrorEnum::UnknownTable {
            table: table.into(),
        })
    }

    /// The refusal of `value` in `table.key`, whose field holds at most `max`.
    #[must_use]
    pub fn out_of_range(table: &str, key: &str, value: i64, max: u64) -> Self {
        Self(ServiceConfigErrorEnum::OutOfRange {
            setting: format!("{table}.{key}"),
            value,
            max,
        })
    }

    /// The refusal of a start lacking `table.key`, which the role requires.
    #[must_use]
    pub fn missing(table: &str, key: &str) -> Self {
        Self(ServiceConfigErrorEnum::Missing {
            setting: format!("{table}.{key}"),
        })
    }
}
