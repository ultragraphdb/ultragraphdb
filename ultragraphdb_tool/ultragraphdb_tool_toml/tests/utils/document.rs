/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A parsed document as rows, and a refusal as its variant, so a test states what it expects as literals.

use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_tool_toml::{TomlDocument, TomlValue};
use ultragraphdb_tool_toml::{TomlErrorEnum, TomlKind};

/// Every entry of `document` as (table, key, value), the root table first, each table's entries in their order.
pub fn rows(document: &TomlDocument) -> Vec<(String, String, TomlValue)> {
    document
        .tables()
        .iter()
        .flat_map(|table| {
            table.entries().iter().map(|entry| {
                (
                    table.name().to_string(),
                    entry.key().to_string(),
                    entry.value().clone(),
                )
            })
        })
        .collect()
}

/// A row of table `table`, key `key` and value `value`.
pub fn row(table: &str, key: &str, value: TomlValue) -> (String, String, TomlValue) {
    (table.to_string(), key.to_string(), value)
}

/// Reads every setting of `document` again through the typed read of its kind: each answers the value the document
/// holds, a read of another kind is refused naming both kinds, and the final check then passes.
pub fn reread_alike(mut document: TomlDocument) -> TestResult {
    for (table, key, value) in rows(&document) {
        let (expected, found) = match &value {
            TomlValue::String(text) => {
                assert_eq!(&document.string(&table, &key)?, text);
                (TomlKind::Integer, document.integer(&table, &key))
            }
            TomlValue::Integer(number) => {
                assert_eq!(document.integer(&table, &key)?, *number);
                (
                    TomlKind::Boolean,
                    document.boolean(&table, &key).map(i64::from),
                )
            }
            TomlValue::Boolean(flag) => {
                assert_eq!(document.boolean(&table, &key)?, *flag);
                (TomlKind::Integer, document.integer(&table, &key))
            }
        };
        let Err(error) = found else {
            return Err(TestError::new(format!("{table}.{key} read as {expected}")));
        };
        assert_eq!(
            error.0,
            TomlErrorEnum::WrongKind {
                table: table.clone(),
                key: key.clone(),
                expected,
                found: value.kind(),
            }
        );
    }
    document.finish()?;
    Ok(())
}
