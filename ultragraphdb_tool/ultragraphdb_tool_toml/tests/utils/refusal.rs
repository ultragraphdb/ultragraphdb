/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests alone ask of a parse: the tables' names, and a refusal as its variant.

use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_tool_toml::{TomlDocument, TomlErrorEnum};

use crate::utils::document::rows;

/// The names of the tables of `document`, the root table's empty name first.
pub fn table_names(document: &TomlDocument) -> Vec<String> {
    document
        .tables()
        .iter()
        .map(|table| table.name().to_string())
        .collect()
}

/// Why `bytes` was refused; a failure when it parses.
pub fn refusal(bytes: &[u8]) -> TestResult<TomlErrorEnum> {
    match TomlDocument::parse(bytes) {
        Ok(document) => Err(TestError::new(format!(
            "{:?} parsed as {:?}, not refused",
            String::from_utf8_lossy(bytes),
            rows(&document)
        ))),
        Err(error) => Ok(error.0),
    }
}
