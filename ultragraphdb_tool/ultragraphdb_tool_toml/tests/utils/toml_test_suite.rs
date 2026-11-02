/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The vendored toml-test documents (`testdata/toml-test/SOURCE.txt`): each valid one compared with its expected JSON,
//! each invalid one refused.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult, testdata_dir};
use ultragraphdb_tool_toml::TomlErrorEnum;
use ultragraphdb_tool_toml::{TomlDocument, TomlValue};

/// The valid documents vendored: those of the suite's 185 that use only the subset (SOURCE.txt).
pub const VALID_DOCUMENTS: usize = 38;
/// The invalid documents vendored: all of the suite's TOML 1.0.0 list (SOURCE.txt).
pub const INVALID_DOCUMENTS: usize = 371;

/// Every file under `dir` whose name ends in `.<extension>`, in path order, walked with an explicit stack.
pub fn files(dir: &Path, extension: &str) -> TestResult<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in fs::read_dir(&next).context("a testdata directory")? {
            let path = entry.context("a directory entry")?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == extension) {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// The suite's directory `kind`, `valid` or `invalid`.
fn suite_dir(kind: &str) -> TestResult<PathBuf> {
    Ok(testdata_dir()?.join("toml-test").join(kind))
}

/// Each valid document: its path, its bytes, and the JSON the suite expects of it.
pub fn valid_cases() -> TestResult<Vec<(PathBuf, Vec<u8>, Value)>> {
    files(&suite_dir("valid")?, "toml")?
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).context("a valid document")?;
            let json = fs::read(path.with_extension("json")).context("its expected JSON")?;
            let expected = serde_json::from_slice(&json).context("the expected JSON")?;
            Ok((path, bytes, expected))
        })
        .collect()
}

/// Each invalid document: its path and its bytes.
pub fn invalid_cases() -> TestResult<Vec<(PathBuf, Vec<u8>)>> {
    files(&suite_dir("invalid")?, "toml")?
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).context("an invalid document")?;
            Ok((path, bytes))
        })
        .collect()
}

/// A value in the suite's JSON form: its type and its text.
fn leaf(value: &TomlValue) -> Value {
    match value {
        TomlValue::String(text) => json!({"type": "string", "value": text}),
        TomlValue::Integer(number) => json!({"type": "integer", "value": number.to_string()}),
        TomlValue::Boolean(flag) => json!({"type": "bool", "value": flag.to_string()}),
    }
}

/// `document` in the suite's JSON form: root keys at the top, each table an object of its keys.
pub fn as_suite_json(document: &TomlDocument) -> Value {
    let mut root = Map::new();
    for table in document.tables() {
        let entries: Map<String, Value> = table
            .entries()
            .iter()
            .map(|entry| (entry.key().to_string(), leaf(entry.value())))
            .collect();
        if table.name().is_empty() {
            root.extend(entries);
        } else {
            let _replaced: Option<Value> =
                root.insert(table.name().to_string(), Value::Object(entries));
        }
    }
    Value::Object(root)
}

/// The valid document at `path` parses to `expected`.
pub fn check_valid(path: &Path, bytes: &[u8], expected: &Value) -> TestResult {
    let document = TomlDocument::parse(bytes)
        .map_err(|e| TestError::new(format!("{} is refused: {e}", path.display())))?;
    let parsed = as_suite_json(&document);
    if &parsed == expected {
        Ok(())
    } else {
        Err(TestError::new(format!(
            "{} parses to {parsed}, not {expected}",
            path.display()
        )))
    }
}

/// The invalid document at `path` is refused as unsupported or as invalid.
pub fn check_invalid(path: &Path, bytes: &[u8]) -> TestResult {
    match TomlDocument::parse(bytes) {
        Ok(document) => Err(TestError::new(format!(
            "{} parses to {}",
            path.display(),
            as_suite_json(&document)
        ))),
        Err(error) => match error.0 {
            TomlErrorEnum::Unsupported { .. } | TomlErrorEnum::Invalid { .. } => Ok(()),
            TomlErrorEnum::Limit { .. }
            | TomlErrorEnum::Missing { .. }
            | TomlErrorEnum::WrongKind { .. }
            | TomlErrorEnum::UnreadKey { .. } => Err(TestError::new(format!(
                "{} is refused, but not as unsupported or invalid: {error}",
                path.display()
            ))),
        },
    }
}
