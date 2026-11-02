/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a settings file: the parser never panics, refusing with its typed error or accepting. An accepted
//! document's values re-read alike: each setting's typed read answers the value the document holds and the final check
//! passes; and the document written again as TOML of the subset parses to the same tables, keys and values.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_tool_toml::{TomlDocument, TomlValue};

/// Every entry as (table, key, value), the root table first.
fn rows(document: &TomlDocument) -> Vec<(String, String, TomlValue)> {
    document
        .tables()
        .iter()
        .flat_map(|t| {
            t.entries()
                .iter()
                .map(|e| (t.name().to_string(), e.key().to_string(), e.value().clone()))
        })
        .collect()
}

/// `text` as a basic string, every control character and DEL escaped.
fn quote(text: &str) -> String {
    let mut quoted = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            c if c < ' ' || c == '\u{7f}' => quoted.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// `document` written as TOML of the subset.
fn write(document: &TomlDocument) -> String {
    let mut text = String::new();
    for table in document.tables() {
        if !table.name().is_empty() {
            text.push_str(&format!("[{}]\n", table.name()));
        }
        for entry in table.entries() {
            let value = match entry.value() {
                TomlValue::String(s) => quote(s),
                TomlValue::Integer(n) => n.to_string(),
                TomlValue::Boolean(b) => b.to_string(),
            };
            text.push_str(&format!("{} = {value}\n", entry.key()));
        }
    }
    text
}

fuzz_target!(|data: &[u8]| {
    let Ok(document) = TomlDocument::parse(data) else {
        return;
    };
    let entries = rows(&document);
    let mut reading = document.clone();
    for (table, key, value) in &entries {
        match value {
            TomlValue::String(s) => assert_eq!(reading.string(table, key).as_ref(), Ok(s)),
            TomlValue::Integer(n) => assert_eq!(reading.integer(table, key), Ok(*n)),
            TomlValue::Boolean(b) => assert_eq!(reading.boolean(table, key), Ok(*b)),
        }
    }
    assert_eq!(reading.finish(), Ok(()), "every setting was read");
    let text = write(&document);
    // Escaping lengthens a string, so a document at a limit may not fit once written; within them it must read back.
    let fits = text.len() <= TomlDocument::BYTES_MAX
        && text
            .lines()
            .all(|line| line.len() <= TomlDocument::LINE_BYTES_MAX);
    if fits {
        match TomlDocument::parse(text.as_bytes()) {
            Ok(again) => assert_eq!(rows(&again), entries, "written as {text:?}"),
            Err(error) => panic!("written as {text:?}, refused: {error}"),
        }
    }
});
