/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::panic;

use cucumber::{World, then, when};
use serde::Deserialize;
use ultragraphdb_core_keys::{
    KeyError, ParsedKey, ReservedNamespace, TrustMarker, count_key, data_key, index_key,
    is_pre_defined_predicate, is_reserved_predicate, namespace_attr, parse, parse_namespace_attr,
    register_reserved_namespace, reserved_predicate_value_lock, reverse_key, schema_key, split_key,
    type_key,
};

#[derive(Debug, Default, World)]
struct Steps {
    key: Vec<u8>,
    uid_keys: Vec<(u64, Vec<u8>)>,
    parsed: Option<Result<ParsedKey, KeyError>>,
    mismatches: Vec<String>,
    compared: usize,
    formatted: String,
    checked: Vec<(String, bool, bool)>,
    markers: Vec<TrustMarker>,
    second_panicked: Option<bool>,
}

/// One case of `testdata/golden/keys.json`, the fields a key builder takes.
#[derive(Debug, Deserialize)]
struct GoldenKey {
    kind: String,
    namespace: u64,
    attr: String,
    #[serde(default)]
    uid: u64,
    #[serde(default)]
    term: String,
    #[serde(default)]
    count: u32,
    #[serde(default)]
    start_uid: u64,
    key: String,
}

fn hex_bytes(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .flat_map(|chunk| {
            (0..chunk.len())
                .step_by(2)
                .map(move |i| u8::from_str_radix(&chunk[i..i + 2], 16).expect("hex"))
        })
        .collect()
}

fn build(case: &GoldenKey) -> Option<Vec<u8>> {
    let attr = namespace_attr(case.namespace, &case.attr);
    let (kind, split) = match case.kind.strip_prefix("split_") {
        Some(kind) => (kind, true),
        None => (case.kind.as_str(), false),
    };
    let key = match kind {
        "data" => data_key(&attr, case.uid),
        "reverse" => reverse_key(&attr, case.uid),
        "index" => index_key(&attr, &hex_bytes(&case.term)),
        "count" => count_key(&attr, case.count, false),
        "count_rev" => count_key(&attr, case.count, true),
        "schema" => schema_key(&attr),
        "type" => type_key(&attr),
        _ => return None,
    };
    Some(if split {
        split_key(&key, case.start_uid).expect("a default-prefix key splits")
    } else {
        key
    })
}

#[when(expr = "the data key for attribute {string} in namespace {int} and uid {int} is built")]
fn build_data_key(w: &mut Steps, attr: String, ns: u64, uid: u64) {
    w.key = data_key(&namespace_attr(ns, &attr), uid);
}

#[then(expr = "it equals the bytes {string}")]
fn equals_bytes(w: &mut Steps, bytes: String) {
    assert_eq!(w.key, hex_bytes(&bytes));
}

#[when(
    "data, index, reverse, count, reverse-count, schema and type keys are built for the golden inputs"
)]
fn build_golden(w: &mut Steps) {
    let path =
        ultragraphdb_test_bdd::testdata_dir("ultragraphdb_core_keys").join("golden/keys.json");
    let golden = ultragraphdb_test_bdd::load_golden::<GoldenKey>(&path);
    for case in golden.cases() {
        if let Some(actual) = build(case) {
            w.compared += 1;
            if let Err(e) = ultragraphdb_test_bdd::compare_bytes(
                &format!("{case:?}"),
                &hex_bytes(&case.key),
                &actual,
            ) {
                w.mismatches.push(e);
            }
        }
    }
}

#[then("each equals the bytes Dgraph produces for the same input")]
fn golden_equal(w: &mut Steps) {
    assert!(
        w.compared > 1000,
        "only {} golden keys compared",
        w.compared
    );
    assert!(w.mismatches.is_empty(), "{}", w.mismatches.join("\n"));
}

#[when(
    expr = "data keys for the attribute {string} are built for uids 1, 2, 256 and 1099511627776"
)]
fn build_uid_keys(w: &mut Steps, attr: String) {
    // Built in an order other than the numeric one, so that sorting has something to do.
    w.uid_keys = [256u64, 1 << 40, 2, 1]
        .iter()
        .map(|&uid| (uid, data_key(&namespace_attr(0, &attr), uid)))
        .collect();
}

#[then("their byte order equals their numeric uid order")]
fn uid_order(w: &mut Steps) {
    let mut by_bytes = w.uid_keys.clone();
    by_bytes.sort_by(|a, b| a.1.cmp(&b.1));
    let uids: Vec<u64> = by_bytes.iter().map(|(uid, _)| *uid).collect();
    assert_eq!(uids, vec![1, 2, 256, 1 << 40]);
}

#[when(expr = "a data key for namespace {int}, attribute {string} and uid {int} is parsed")]
fn parse_data_key(w: &mut Steps, ns: u64, attr: String, uid: u64) {
    w.parsed = Some(parse(&data_key(&namespace_attr(ns, &attr), uid)));
}

#[then(expr = "the result is a data key with namespace {int}, attribute {string} and uid {int}")]
fn parsed_data_key(w: &mut Steps, ns: u64, attr: String, uid: u64) {
    let pk = w.parsed.take().expect("parsed").expect("parses");
    assert!(pk.is_data());
    assert_eq!(parse_namespace_attr(pk.attr()), (ns, attr.as_str()));
    assert_eq!(pk.uid(), uid);
}

#[when(expr = "a data key for attribute {string} carrying uid 0 is parsed")]
fn parse_uid_zero(w: &mut Steps, attr: String) {
    w.key = data_key(&namespace_attr(0, &attr), 0);
    w.parsed = Some(parse(&w.key));
}

#[then(expr = "parsing fails with {string} followed by the key after its namespace")]
fn fails_with_key(w: &mut Steps, message: String) {
    let err = w
        .parsed
        .take()
        .expect("parsed")
        .expect_err("uid 0 is rejected");
    // Dgraph prints the key past its prefix byte and namespace, as Go prints a byte slice.
    let rest: Vec<String> = w.key[9..].iter().map(u8::to_string).collect();
    assert_eq!(err.to_string(), format!("{message} [{}]", rest.join(" ")));
}

#[when(expr = "a data key for attribute {string} in namespace {int} is built and parsed")]
fn build_and_parse(w: &mut Steps, attr: String, ns: u64) {
    w.parsed = Some(parse(&data_key(&namespace_attr(ns, &attr), 1)));
}

#[then(expr = "the parsed attribute is {string} and the namespace is {int}")]
fn parsed_attr(w: &mut Steps, attr: String, ns: u64) {
    let pk = w.parsed.take().expect("parsed").expect("parses");
    assert_eq!(parse_namespace_attr(pk.attr()), (ns, attr.as_str()));
}

fn split_data_key(attr: &str, uid: u64, start: u64) -> Vec<u8> {
    split_key(&data_key(&namespace_attr(0, attr), uid), start).expect("a data key splits")
}

#[when(
    expr = "the split key for the data key of attribute {string}, uid {int} and start uid {int} is built and parsed"
)]
fn parse_split(w: &mut Steps, attr: String, uid: u64, start: u64) {
    w.parsed = Some(parse(&split_data_key(&attr, uid, start)));
}

#[then(expr = "the parse is a data key with attribute {string}, uid {int} and start uid {int}")]
fn parsed_split(w: &mut Steps, attr: String, uid: u64, start: u64) {
    let pk = w.parsed.take().expect("parsed").expect("parses");
    assert!(pk.is_data());
    assert!(pk.has_start_uid());
    assert_eq!(parse_namespace_attr(pk.attr()), (0, attr.as_str()));
    assert_eq!((pk.uid(), pk.start_uid()), (uid, start));
}

#[when(
    expr = "one byte is appended to the split key for the data key of attribute {string}, uid {int} and start uid {int}"
)]
fn parse_split_trailing(w: &mut Steps, attr: String, uid: u64, start: u64) {
    let mut key = split_data_key(&attr, uid, start);
    key.push(0);
    w.parsed = Some(parse(&key));
}

#[then("parsing it fails")]
fn parsing_fails(w: &mut Steps) {
    assert!(w.parsed.take().expect("parsed").is_err());
}

#[when(expr = "namespace {int} and predicate {string} are combined")]
fn combine(w: &mut Steps, ns: u64, pred: String) {
    w.formatted = namespace_attr(ns, &pred);
}

#[then(
    expr = "the result is {string}, and parsing it returns namespace {int} and predicate {string}"
)]
fn combined(w: &mut Steps, formatted: String, ns: u64, pred: String) {
    assert_eq!(w.formatted, formatted);
    assert_eq!(parse_namespace_attr(&w.formatted), (ns, pred.as_str()));
}

#[when(expr = "{string} and {string} are checked")]
fn check_names(w: &mut Steps, a: String, b: String) {
    w.checked = [a, b]
        .into_iter()
        .map(|n| {
            let (reserved, pre_defined) = (is_reserved_predicate(&n), is_pre_defined_predicate(&n));
            (n, reserved, pre_defined)
        })
        .collect();
}

#[then(expr = "both are reserved, and only {string} is pre-defined")]
fn reserved_not_pre_defined(w: &mut Steps, only: String) {
    for (name, reserved, pre_defined) in &w.checked {
        assert!(*reserved, "{name} is reserved");
        assert_eq!(*pre_defined, *name == only, "{name} pre-defined");
    }
}

#[when(expr = "plugins register the value-locked prefixes {string} and {string}")]
fn register_prefixes(w: &mut Steps, first: String, second: String) {
    w.markers = vec![
        TrustMarker::new("bdd-first"),
        TrustMarker::new("bdd-second"),
    ];
    for (prefix, marker) in [first, second].iter().zip(&w.markers) {
        register_reserved_namespace(
            ReservedNamespace::new()
                .with_predicate_prefix(prefix)
                .with_value_locked_prefixes([prefix.as_str()])
                .with_trust_marker(*marker),
        );
    }
}

#[then(expr = "{string} belongs to the second and {string} to the first")]
fn longest_prefix(w: &mut Steps, second: String, first: String) {
    assert_eq!(reserved_predicate_value_lock(&second), Some(w.markers[1]));
    assert_eq!(reserved_predicate_value_lock(&first), Some(w.markers[0]));
}

#[when(expr = "two plugins register the exact name {string}")]
fn register_twice(w: &mut Steps, name: String) {
    let plugin = || ReservedNamespace::new().with_predicates([name.as_str()]);
    register_reserved_namespace(plugin());
    w.second_panicked =
        Some(panic::catch_unwind(|| register_reserved_namespace(plugin())).is_err());
}

#[then("the second registration panics")]
fn second_panics(w: &mut Steps) {
    assert_eq!(w.second_panicked, Some(true));
}

fn main() {
    futures::executor::block_on(Steps::cucumber().fail_on_skipped().run_and_exit(
        ultragraphdb_test_bdd::features_dir("ultragraphdb_core_keys"),
    ));
}
