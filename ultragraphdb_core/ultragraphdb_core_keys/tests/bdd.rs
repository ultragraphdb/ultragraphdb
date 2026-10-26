/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of the quad keys: their encoding, parse and refusals, their order, and their regions.

use cucumber::{World, then, when};
use oxrdf::Literal;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey, QuadKeyError, Region};

/// A quad key's parse: its fields and a split part's start, or why it was refused.
type QuadParse = Result<(QuadKey, Option<u64>), QuadKeyError>;

#[derive(Debug, Default, World)]
struct Steps {
    key: Vec<u8>,
    golden_files: Vec<std::path::PathBuf>,
    quad: Option<QuadParse>,
    quad_keys: Vec<Vec<u8>>,
    regions: Vec<Region>,
    prefix_results: Vec<(Vec<u8>, QuadParse)>,
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

#[when("the key tests run")]
fn key_tests_run(w: &mut Steps) {
    w.golden_files = std::fs::read_dir(ultragraphdb_test_bdd::testdata_dir().join("golden"))
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
}

#[then(
    "they check the quad layout against its specification and read no golden file generated from Dgraph"
)]
fn no_golden_files(w: &mut Steps) {
    assert!(w.golden_files.is_empty(), "{:?}", w.golden_files);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
