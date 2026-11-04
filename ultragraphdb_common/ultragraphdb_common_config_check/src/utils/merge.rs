/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_tool_toml::{TomlDocument, TomlValue};

use crate::types::instance::Instance;
use crate::types::target_files::TargetFiles;

/// The roles a target's instances play.
pub(crate) const ROLES: [&str; 3] = ["frontend", "compute", "storage"];

/// The role `key` names: a role alone, or a role, `_` and a label.
pub(crate) fn role_of(key: &str) -> Option<&'static str> {
    ROLES.iter().copied().find(|role| {
        key == *role
            || key
                .strip_prefix(role)
                .and_then(|rest| rest.strip_prefix('_'))
                .is_some_and(|label| {
                    !label.is_empty()
                        && label.bytes().all(|byte| {
                            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                        })
                })
    })
}

/// Each instance of `files`: a labelled name is one; a role's name is the file its labelled instances share, and is an
/// instance itself when the role has none. An instance's file stands over its role's, key by key.
#[must_use]
pub fn instances(files: &TargetFiles) -> Vec<Instance> {
    let keys: Vec<&str> = files.files().map(|(key, _, _)| key).collect();
    keys.iter()
        .filter_map(|key| role_of(key).map(|role| (*key, role)))
        .filter(|(key, role)| {
            *key != *role
                || !keys
                    .iter()
                    .any(|other| *other != *key && role_of(other) == Some(role))
        })
        .map(|(key, role)| instance(files, key, role))
        .collect()
}

fn instance(files: &TargetFiles, key: &str, role: &'static str) -> Instance {
    let mut merged: BTreeMap<String, BTreeMap<String, TomlValue>> = BTreeMap::new();
    let mut parsed = true;
    let layers = [
        (key != role).then(|| files.bytes(role)).flatten(),
        files.bytes(key),
    ];
    for bytes in layers.into_iter().flatten() {
        match TomlDocument::parse(bytes) {
            Ok(document) => {
                for table in document.tables() {
                    let held = merged.entry(table.name().to_string()).or_default();
                    for entry in table.entries() {
                        let _previous = held.insert(entry.key().to_string(), entry.value().clone());
                    }
                }
            }
            Err(_) => parsed = false,
        }
    }
    let rendered = render(&merged);
    let config = parsed
        .then(|| ServiceConfig::parse(Some(rendered.as_bytes()), []).ok())
        .flatten();
    Instance {
        name: key.to_string(),
        role,
        rendered,
        config,
    }
}

/// `merged` as canonical TOML: tables and keys in order, each value as the subset writes it.
fn render(merged: &BTreeMap<String, BTreeMap<String, TomlValue>>) -> String {
    let mut text = String::new();
    for (table, entries) in merged {
        if entries.is_empty() {
            continue;
        }
        if !table.is_empty() {
            let _written = writeln!(text, "[{table}]");
        }
        for (key, value) in entries {
            let _written = match value {
                TomlValue::String(string) => writeln!(text, "{key} = \"{}\"", escaped(string)),
                TomlValue::Integer(number) => writeln!(text, "{key} = {number}"),
                TomlValue::Boolean(flag) => writeln!(text, "{key} = {flag}"),
            };
        }
    }
    text
}

/// `text` as a basic string's contents.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                let _written = write!(out, "\\u{:04X}", u32::from(control));
            }
            plain => out.push(plain),
        }
    }
    out
}
