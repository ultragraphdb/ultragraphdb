/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::toml_error::{TomlError, TomlErrorEnum};

/// A setting as an operator writes it: `table.key`, or the key alone in the root table.
fn setting(table: &str, key: &str) -> String {
    if table.is_empty() {
        key.to_string()
    } else {
        format!("{table}.{key}")
    }
}

impl fmt::Display for TomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TomlErrorEnum::Unsupported { feature, line } => write!(
                f,
                "line {line}: {feature} are outside the TOML subset settings files use"
            ),
            TomlErrorEnum::Invalid { reason, line } => {
                write!(f, "line {line}: invalid TOML: {reason}")
            }
            TomlErrorEnum::Limit { limit, max } => {
                write!(f, "the settings file passes its limit of {max} {limit}")
            }
            TomlErrorEnum::Missing {
                table,
                key,
                expected,
            } => write!(
                f,
                "the setting {} is missing; {expected} is expected",
                setting(table, key)
            ),
            TomlErrorEnum::WrongKind {
                table,
                key,
                expected,
                found,
            } => write!(
                f,
                "the setting {} must be {expected}, not {found}",
                setting(table, key)
            ),
            TomlErrorEnum::UnreadKey { table, key } => {
                write!(f, "the setting {} is unknown", setting(table, key))
            }
        }
    }
}
