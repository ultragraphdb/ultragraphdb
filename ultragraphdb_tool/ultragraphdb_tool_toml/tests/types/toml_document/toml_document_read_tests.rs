/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The typed reads and the final check that every setting was taken.

use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_tool_toml::TomlDocument;
use ultragraphdb_tool_toml::{TomlErrorEnum, TomlKind};

const SETTINGS: &[u8] =
    b"name = \"dev\"\nverbose = false\n[storage]\npartition_count = 4\nsync = true\npath = \"/data\"\n";

/// The variant of an error that was expected; a failure for a value.
fn refused<T: std::fmt::Debug>(
    read: Result<T, ultragraphdb_tool_toml::TomlError>,
) -> TestResult<TomlErrorEnum> {
    match read {
        Ok(value) => Err(TestError::new(format!("read {value:?}, not refused"))),
        Err(error) => Ok(error.0),
    }
}

#[test]
fn test_each_kind_is_read_from_the_root_table_and_a_named_table() -> TestResult {
    let mut document = TomlDocument::parse(SETTINGS)?;
    assert_eq!(document.string("", "name")?, "dev");
    assert!(!document.boolean("", "verbose")?);
    assert_eq!(document.integer("storage", "partition_count")?, 4);
    assert!(document.boolean("storage", "sync")?);
    assert_eq!(document.string("storage", "path")?, "/data");
    document.finish()?;
    Ok(())
}

#[test]
fn test_a_setting_may_be_read_twice() -> TestResult {
    let mut document = TomlDocument::parse(b"[storage]\npartition_count = 4\n")?;
    assert_eq!(document.integer("storage", "partition_count")?, 4);
    assert_eq!(document.integer("storage", "partition_count")?, 4);
    document.finish()?;
    Ok(())
}

#[test]
fn test_an_absent_key_or_table_is_missing() -> TestResult {
    let mut document = TomlDocument::parse(SETTINGS)?;
    assert_eq!(
        refused(document.integer("storage", "partition"))?,
        TomlErrorEnum::Missing {
            table: "storage".to_string(),
            key: "partition".to_string(),
            expected: TomlKind::Integer,
        }
    );
    assert_eq!(
        refused(document.string("transport", "name"))?,
        TomlErrorEnum::Missing {
            table: "transport".to_string(),
            key: "name".to_string(),
            expected: TomlKind::String,
        }
    );
    // A key of a named table is not a key of the root table, nor the reverse.
    assert_eq!(
        refused(document.boolean("", "sync"))?,
        TomlErrorEnum::Missing {
            table: String::new(),
            key: "sync".to_string(),
            expected: TomlKind::Boolean,
        }
    );
    assert_eq!(
        refused(document.string("storage", "name"))?,
        TomlErrorEnum::Missing {
            table: "storage".to_string(),
            key: "name".to_string(),
            expected: TomlKind::String,
        }
    );
    Ok(())
}

#[test]
fn test_a_value_of_another_kind_is_refused_naming_both_kinds() -> TestResult {
    let mut document = TomlDocument::parse(SETTINGS)?;
    let cases = [
        (
            refused(document.integer("", "name"))?,
            "",
            "name",
            TomlKind::Integer,
            TomlKind::String,
        ),
        (
            refused(document.string("", "verbose"))?,
            "",
            "verbose",
            TomlKind::String,
            TomlKind::Boolean,
        ),
        (
            refused(document.boolean("storage", "partition_count"))?,
            "storage",
            "partition_count",
            TomlKind::Boolean,
            TomlKind::Integer,
        ),
        (
            refused(document.integer("storage", "sync"))?,
            "storage",
            "sync",
            TomlKind::Integer,
            TomlKind::Boolean,
        ),
        (
            refused(document.boolean("storage", "path"))?,
            "storage",
            "path",
            TomlKind::Boolean,
            TomlKind::String,
        ),
        (
            refused(document.string("storage", "partition_count"))?,
            "storage",
            "partition_count",
            TomlKind::String,
            TomlKind::Integer,
        ),
    ];
    for (error, table, key, expected, found) in cases {
        assert_eq!(
            error,
            TomlErrorEnum::WrongKind {
                table: table.to_string(),
                key: key.to_string(),
                expected,
                found,
            }
        );
    }
    Ok(())
}

#[test]
fn test_finish_refuses_the_first_untaken_setting_in_the_documents_order() -> TestResult {
    let mut document = TomlDocument::parse(SETTINGS)?;
    assert_eq!(document.string("", "name")?, "dev");
    assert_eq!(document.integer("storage", "partition_count")?, 4);
    assert_eq!(
        refused(document.finish())?,
        TomlErrorEnum::UnreadKey {
            table: String::new(),
            key: "verbose".to_string(),
        }
    );
    let mut later = TomlDocument::parse(SETTINGS)?;
    assert_eq!(later.string("", "name")?, "dev");
    assert!(!later.boolean("", "verbose")?);
    assert!(later.boolean("storage", "sync")?);
    assert_eq!(
        refused(later.finish())?,
        TomlErrorEnum::UnreadKey {
            table: "storage".to_string(),
            key: "partition_count".to_string(),
        }
    );
    Ok(())
}

#[test]
fn test_a_read_of_the_wrong_kind_takes_nothing() -> TestResult {
    let mut document = TomlDocument::parse(b"[storage]\npartition_count = \"4\"\n")?;
    assert!(document.integer("storage", "partition_count").is_err());
    assert_eq!(
        refused(document.finish())?,
        TomlErrorEnum::UnreadKey {
            table: "storage".to_string(),
            key: "partition_count".to_string(),
        }
    );
    Ok(())
}

#[test]
fn test_reading_one_tables_key_leaves_the_same_key_of_another_untaken() -> TestResult {
    let mut document = TomlDocument::parse(b"sync = true\n[storage]\nsync = false\n")?;
    assert!(document.boolean("", "sync")?);
    assert_eq!(
        refused(document.finish())?,
        TomlErrorEnum::UnreadKey {
            table: "storage".to_string(),
            key: "sync".to_string(),
        }
    );
    Ok(())
}

#[test]
fn test_a_document_without_keys_finishes_with_nothing_read() -> TestResult {
    TomlDocument::parse(b"# nothing\n[storage]\n")?.finish()?;
    Ok(())
}
