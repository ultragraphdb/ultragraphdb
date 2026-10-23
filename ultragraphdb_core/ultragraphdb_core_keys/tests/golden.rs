/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The golden files of `tools/golden/keys.go`, as the tests read them. Go leaves out zero and empty fields.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use ultragraphdb_test_bdd::{Golden, load_golden, testdata_dir};

#[derive(Debug, Deserialize)]
pub struct ParsedCase {
    pub attr: String,
    #[serde(default)]
    pub uid: u64,
    #[serde(default)]
    pub start_uid: u64,
    #[serde(default)]
    pub term: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub byte_type: u8,
    #[serde(default)]
    pub has_start_uid: bool,
    #[serde(default)]
    pub is_data: bool,
    #[serde(default)]
    pub is_reverse: bool,
    #[serde(default)]
    pub is_count: bool,
    #[serde(default)]
    pub is_count_rev: bool,
    #[serde(default)]
    pub is_index: bool,
    #[serde(default)]
    pub is_schema: bool,
    #[serde(default)]
    pub is_type: bool,
    pub string: String,
    #[serde(default)]
    pub skip_predicate: String,
    #[serde(default)]
    pub data_prefix: String,
    #[serde(default)]
    pub index_prefix: String,
    #[serde(default)]
    pub reverse_prefix: String,
    #[serde(default)]
    pub count_prefix: String,
    #[serde(default)]
    pub count_rev_prefix: String,
}

#[derive(Debug, Deserialize)]
pub struct KeyCase {
    pub kind: String,
    pub namespace: u64,
    pub attr: String,
    #[serde(default)]
    pub uid: u64,
    #[serde(default)]
    pub term: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub start_uid: u64,
    pub key: String,
    pub parsed: Option<ParsedCase>,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub panics: bool,
}

#[derive(Debug, Deserialize)]
pub struct ParseErrorCase {
    pub name: String,
    pub key: String,
    pub parsed: Option<ParsedCase>,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub panics: bool,
}

#[derive(Debug, Deserialize)]
pub struct SplitErrorCase {
    pub name: String,
    pub key: String,
    pub start_uid: u64,
    pub split: String,
    pub error: String,
}

#[derive(Debug, Deserialize)]
pub struct ParseErrorFile {
    pub parse: Vec<ParseErrorCase>,
    pub split: Vec<SplitErrorCase>,
}

#[derive(Debug, Deserialize)]
pub struct NamespaceAttrCase {
    pub namespace: u64,
    pub attr: String,
    pub formatted: String,
    pub parsed_namespace: u64,
    pub parsed_attr: String,
    pub namespace_bytes: String,
    pub is_reverse_attr: bool,
    pub is_reverse_attr_panics: bool,
}

#[derive(Debug, Deserialize)]
pub struct ReservedCase {
    pub name: String,
    pub is_reserved_predicate: bool,
    pub is_pre_defined_predicate: bool,
    pub is_acl_predicate: bool,
    pub is_other_reserved_predicate: bool,
    pub is_internal_predicate: bool,
    pub is_reserved_type: bool,
    pub is_pre_defined_type: bool,
    pub is_registered_reserved_predicate: bool,
}

/// The golden file `testdata/golden/<name>.json`.
pub fn golden<T: DeserializeOwned>(name: &str) -> Golden<T> {
    load_golden::<T>(&testdata_dir().join(format!("golden/{name}.json")))
}

/// The parse-error file, whose `cases` is an object rather than a list.
pub fn parse_errors() -> ParseErrorFile {
    let path = testdata_dir().join("golden/keys_parse_errors.json");
    let text = std::fs::read_to_string(path).expect("golden file");
    let value: serde_json::Value = serde_json::from_str(&text).expect("golden json");
    assert!(
        value["dgraph_commit"]
            .as_str()
            .is_some_and(|c| !c.is_empty())
    );
    serde_json::from_value(value["cases"].clone()).expect("golden cases")
}

pub fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

pub fn compare(input: &str, expected_hex: &str, actual: &[u8]) {
    if let Err(e) = ultragraphdb_test_bdd::compare_bytes(input, &unhex(expected_hex), actual) {
        panic!("{e}");
    }
}
