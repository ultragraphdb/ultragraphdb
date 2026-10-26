/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use cucumber::{World, then, when};
use oxrdf::Literal;
use serde::Deserialize;
use ultragraphdb_core_keys::{
    DEFAULT_GRAPH, KeyError, KeyErrorEnum, ParsedKey, QuadKey, QuadKeyError, Region,
    ReservedNamespace, TrustMarker, count_key, data_key, index_key, is_pre_defined_predicate,
    is_reserved_predicate, namespace_attr, parse, parse_namespace_attr, predicate_prefix,
    register_reserved_namespace, reserved_predicate_value_lock, reverse_key, schema_key, split_key,
    type_key,
};

/// A quad key's parse: its fields and a split part's start, or why it was refused.
type QuadParse = Result<(QuadKey, Option<u64>), QuadKeyError>;

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
    second_refusal: Option<String>,
    quad: Option<QuadParse>,
    quad_keys: Vec<Vec<u8>>,
    regions: Vec<Region>,
    prefix_results: Vec<(Vec<u8>, QuadParse)>,
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
        "data" => data_key(&attr, case.uid).unwrap(),
        "reverse" => reverse_key(&attr, case.uid).unwrap(),
        "index" => index_key(&attr, &hex_bytes(&case.term)).unwrap(),
        "count" => count_key(&attr, case.count, false).unwrap(),
        "count_rev" => count_key(&attr, case.count, true).unwrap(),
        "schema" => schema_key(&attr).unwrap(),
        "type" => type_key(&attr).unwrap(),
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
    w.key = data_key(&namespace_attr(ns, &attr), uid).unwrap();
}

#[then(expr = "it equals the bytes {string}")]
fn equals_bytes(w: &mut Steps, bytes: String) {
    assert_eq!(w.key, hex_bytes(&bytes));
}

#[when(
    "data, index, reverse, count, reverse-count, schema and type keys are built for the golden inputs"
)]
fn build_golden(w: &mut Steps) {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/keys.json");
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
        .map(|&uid| (uid, data_key(&namespace_attr(0, &attr), uid).unwrap()))
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
    w.parsed = Some(parse(&data_key(&namespace_attr(ns, &attr), uid).unwrap()));
}

#[then(expr = "the result is a data key with namespace {int}, attribute {string} and uid {int}")]
fn parsed_data_key(w: &mut Steps, ns: u64, attr: String, uid: u64) {
    let pk = w.parsed.take().expect("parsed").expect("parses");
    assert!(pk.is_data());
    assert_eq!(
        parse_namespace_attr(pk.attr()).unwrap(),
        (ns, attr.as_str())
    );
    assert_eq!(pk.uid(), uid);
}

#[when(expr = "a data key for attribute {string} carrying uid 0 is parsed")]
fn parse_uid_zero(w: &mut Steps, attr: String) {
    w.key = data_key(&namespace_attr(0, &attr), 0).unwrap();
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
    w.parsed = Some(parse(&data_key(&namespace_attr(ns, &attr), 1).unwrap()));
}

#[then(expr = "the parsed attribute is {string} and the namespace is {int}")]
fn parsed_attr(w: &mut Steps, attr: String, ns: u64) {
    let pk = w.parsed.take().expect("parsed").expect("parses");
    assert_eq!(
        parse_namespace_attr(pk.attr()).unwrap(),
        (ns, attr.as_str())
    );
}

fn split_data_key(attr: &str, uid: u64, start: u64) -> Vec<u8> {
    split_key(&data_key(&namespace_attr(0, attr), uid).unwrap(), start).expect("a data key splits")
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
    assert_eq!(parse_namespace_attr(pk.attr()).unwrap(), (0, attr.as_str()));
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
    // A split data key's start uid must be exactly 8 bytes; Dgraph's text names it.
    let err = w
        .parsed
        .take()
        .expect("parsed")
        .expect_err("the trailing byte is refused");
    assert!(
        matches!(err.0, KeyErrorEnum::StartUidLength { .. }),
        "{err:?}"
    );
    assert!(
        err.to_string()
            .starts_with("StartUid length != 8 for key: "),
        "{err}"
    );
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
    assert_eq!(
        parse_namespace_attr(&w.formatted).unwrap(),
        (ns, pred.as_str())
    );
}

#[when(expr = "{string} and {string} are checked")]
fn check_names(w: &mut Steps, a: String, b: String) {
    w.checked = [a, b]
        .into_iter()
        .map(|n| {
            let (reserved, pre_defined) = (
                is_reserved_predicate(&n).unwrap(),
                is_pre_defined_predicate(&n).unwrap(),
            );
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
        )
        .unwrap();
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
    register_reserved_namespace(plugin()).expect("the first registration is accepted");
    w.second_refusal = register_reserved_namespace(plugin())
        .err()
        .map(|e| e.to_string());
}

#[then(expr = "the second registration is refused with {string}")]
fn second_refused(w: &mut Steps, message: String) {
    assert_eq!(w.second_refusal.as_deref(), Some(message.as_str()));
}

#[when(expr = "the predicate prefix of {string} in namespace {int} is parsed as a key")]
fn parse_predicate_prefix(w: &mut Steps, attr: String, ns: u64) {
    w.key = predicate_prefix(&namespace_attr(ns, &attr)).expect("the prefix builds");
    w.parsed = Some(parse(&w.key));
}

#[then(expr = "parsing fails with {string} and nothing panics")]
fn fails_without_panic(w: &mut Steps, message: String) {
    let err = w
        .parsed
        .take()
        .expect("parsed")
        .expect_err("the prefix is refused");
    assert!(err.to_string().starts_with(&message), "{err}");
}

fn forward(subject: u64) -> QuadKey {
    QuadKey::Forward {
        namespace: 133,
        graph: DEFAULT_GRAPH,
        predicate: 7,
        subject,
    }
}

fn encode_literal(literal: &Literal) -> ultragraphdb_core_terms::EncodedTerm {
    ultragraphdb_core_terms::EncodedTerm::literal(literal.as_ref(), |_| {
        Ok::<_, ultragraphdb_core_terms::TermError>(1)
    })
    .expect("encodes")
    .into_term()
}

#[when(
    "a forward key for namespace 133, the default graph, predicate UID 7 and subject UID 42 is parsed"
)]
fn parse_forward(w: &mut Steps) {
    w.quad = Some(QuadKey::parse(&forward(42).to_bytes()));
}

#[then("the result is a forward key with namespace 133, graph 0, predicate 7 and subject 42")]
fn forward_parsed(w: &mut Steps) {
    assert_eq!(w.quad.take(), Some(Ok((forward(42), None))));
}

#[when("a forward key carrying subject UID 0 is parsed")]
fn parse_zero_subject(w: &mut Steps) {
    w.key = forward(0).to_bytes();
    w.quad = Some(QuadKey::parse(&w.key));
}

#[then("parsing fails with an error naming the key's bytes")]
fn fails_naming_key(w: &mut Steps) {
    let error = w.quad.take().expect("parsed").expect_err("refused");
    let hex: String = w.key.iter().map(|b| format!("{b:02x}")).collect();
    assert!(error.to_string().contains(&hex), "{error}");
}

#[when("a reverse key whose object is the encoding of \"Ann\"@en is built and parsed")]
fn parse_reverse_literal(w: &mut Steps) {
    let key = QuadKey::Reverse {
        namespace: 0,
        graph: DEFAULT_GRAPH,
        predicate: 9,
        object: encode_literal(&Literal::new_language_tagged_literal("Ann", "en").unwrap()),
    };
    w.quad = Some(QuadKey::parse(&key.to_bytes()));
}

#[then("the parsed object decodes to \"Ann\"@en")]
fn reverse_object_decodes(w: &mut Steps) {
    let Some(Ok((QuadKey::Reverse { object, .. }, None))) = w.quad.take() else {
        panic!("a reverse key");
    };
    let literal = object
        .view()
        .unwrap()
        .to_literal(
            |_| -> Result<oxrdf::NamedNode, ultragraphdb_core_terms::TermError> {
                panic!("no UID")
            },
            |_| panic!("no hash"),
        )
        .unwrap();
    assert_eq!(
        literal,
        Literal::new_language_tagged_literal("Ann", "en").unwrap()
    );
}

#[when("the split key for a forward key and start 1000 is built and parsed")]
fn parse_split_forward(w: &mut Steps) {
    w.quad = Some(QuadKey::parse(&forward(42).split_bytes(1000).unwrap()));
}

#[then("the parse returns the base key's parts and start 1000")]
fn split_parsed(w: &mut Steps) {
    assert_eq!(w.quad.take(), Some(Ok((forward(42), Some(1000)))));
}

#[when("one byte is appended to a valid split forward key and the result is parsed")]
fn parse_quad_split_trailing(w: &mut Steps) {
    let mut key = forward(42).split_bytes(1000).unwrap();
    key.push(0);
    w.quad = Some(QuadKey::parse(&key));
}

#[then("parsing fails")]
fn quad_parsing_fails(w: &mut Steps) {
    assert!(w.quad.take().expect("parsed").is_err());
}

#[when("every proper prefix of a forward, reverse, value-index and statement key is parsed")]
fn parse_prefixes(w: &mut Steps) {
    let keys = [
        forward(42),
        QuadKey::Reverse {
            namespace: 1,
            graph: 3,
            predicate: 9,
            object: encode_literal(&Literal::new_simple_literal("a long string, hashed")),
        },
        QuadKey::ValueIndex {
            namespace: 1,
            graph: 3,
            predicate: 9,
            tokenizer: 2,
            token: b"abc".to_vec(),
        },
        QuadKey::Statement {
            namespace: 1,
            graph: 3,
            statement: 77,
        },
    ];
    for key in keys {
        let bytes = key.to_bytes();
        for end in 0..bytes.len() {
            let prefix = bytes[..end].to_vec();
            let result =
                std::panic::catch_unwind(|| QuadKey::parse(&prefix)).expect("parse never panics");
            w.prefix_results.push((prefix, result));
        }
    }
}

#[then("each parse fails with an error, and nothing panics")]
fn prefixes_fail(w: &mut Steps) {
    assert!(w.prefix_results.len() > 100);
    for (prefix, result) in &w.prefix_results {
        match result {
            Err(_) => {}
            // A value-index key with a shorter token is itself a valid key: its token, the last field, may be empty.
            Ok((QuadKey::ValueIndex { token, .. }, None)) => {
                assert!(
                    prefix.len() >= 26 && b"abc".starts_with(token),
                    "{prefix:?}"
                )
            }
            Ok(other) => panic!("prefix {prefix:?} parsed as {other:?}"),
        }
    }
}

#[when("forward keys of one predicate are built for subjects with UIDs 1, 2, 256 and 2^40")]
fn build_forward_keys(w: &mut Steps) {
    w.quad_keys = [1u64, 2, 256, 1 << 40]
        .map(|s| forward(s).to_bytes())
        .to_vec();
}

#[then("their byte order equals the UIDs' numeric order")]
fn forward_keys_sorted(w: &mut Steps) {
    assert!(w.quad_keys.windows(2).all(|p| p[0] < p[1]));
}

#[when("the forward keys of knows for a and for c in the default graph are mapped to regions")]
fn map_regions(w: &mut Steps) {
    let (knows, name, a, c) = (11, 12, 21, 23);
    let key = |predicate, subject| QuadKey::Forward {
        namespace: 0,
        graph: DEFAULT_GRAPH,
        predicate,
        subject,
    };
    w.regions = [key(knows, a), key(knows, c), key(name, a)]
        .iter()
        .map(|k| Region::of(&k.to_bytes()).unwrap())
        .collect();
}

#[then("both map to the same region, and the forward key of name for a maps to another")]
fn regions_shared(w: &mut Steps) {
    assert_eq!(w.regions[0], w.regions[1]);
    assert_ne!(w.regions[0], w.regions[2]);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
