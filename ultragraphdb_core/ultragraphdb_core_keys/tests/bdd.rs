/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of the quad keys: their encoding, parse and refusals, their order, and their regions.

use cucumber::{World, then, when};
use oxrdf::Literal;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey, QuadKeyError, Region};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

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

fn encode_literal(literal: &Literal) -> TestResult<ultragraphdb_core_terms::EncodedTerm> {
    Ok(
        ultragraphdb_core_terms::EncodedTerm::literal(literal.as_ref(), |_| {
            Ok::<_, ultragraphdb_core_terms::TermError>(1)
        })
        .context("encodes")?
        .into_term(),
    )
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
fn fails_naming_key(w: &mut Steps) -> TestResult {
    let Err(error) = w.quad.take().context("parsed")? else {
        return Err(TestError::new("refused"));
    };
    let hex: String = w.key.iter().map(|b| format!("{b:02x}")).collect();
    assert!(error.to_string().contains(&hex), "{error}");
    Ok(())
}

#[when("a reverse key whose object is the encoding of \"Ann\"@en is built and parsed")]
fn parse_reverse_literal(w: &mut Steps) -> TestResult {
    let key = QuadKey::Reverse {
        namespace: 0,
        graph: DEFAULT_GRAPH,
        predicate: 9,
        object: encode_literal(&Literal::new_language_tagged_literal("Ann", "en")?)?,
    };
    w.quad = Some(QuadKey::parse(&key.to_bytes()));
    Ok(())
}

#[then("the parsed object decodes to \"Ann\"@en")]
fn reverse_object_decodes(w: &mut Steps) -> TestResult {
    let quad = w.quad.take();
    let object = if let Some(Ok((QuadKey::Reverse { object, .. }, None))) = &quad {
        Some(object)
    } else {
        None
    };
    let object = object
        .ok_or_else(|| format!("a reverse key, not {quad:?}"))
        .context("the key parses as a reverse key")?;
    // The literal is inline: neither lookup may be asked.
    let literal = object.view()?.to_literal(
        |_| {
            Err(ultragraphdb_core_terms::TermError::malformed(
                Vec::new(),
                "no UID lookup",
            ))
        },
        |_| {
            Err(ultragraphdb_core_terms::TermError::malformed(
                Vec::new(),
                "no hash lookup",
            ))
        },
    )?;
    assert_eq!(literal, Literal::new_language_tagged_literal("Ann", "en")?);
    Ok(())
}

#[when("the split key for a forward key and start 1000 is built and parsed")]
fn parse_split_forward(w: &mut Steps) -> TestResult {
    w.quad = Some(QuadKey::parse(&forward(42).split_bytes(1000)?));
    Ok(())
}

#[then("the parse returns the base key's parts and start 1000")]
fn split_parsed(w: &mut Steps) {
    assert_eq!(w.quad.take(), Some(Ok((forward(42), Some(1000)))));
}

#[when("one byte is appended to a valid split forward key and the result is parsed")]
fn parse_quad_split_trailing(w: &mut Steps) -> TestResult {
    let mut key = forward(42).split_bytes(1000)?;
    key.push(0);
    w.quad = Some(QuadKey::parse(&key));
    Ok(())
}

#[then("parsing fails")]
fn quad_parsing_fails(w: &mut Steps) -> TestResult {
    assert!(w.quad.take().context("parsed")?.is_err());
    Ok(())
}

#[when("every proper prefix of a forward, reverse, value-index and statement key is parsed")]
fn parse_prefixes(w: &mut Steps) -> TestResult {
    let keys = [
        forward(42),
        QuadKey::Reverse {
            namespace: 1,
            graph: 3,
            predicate: 9,
            object: encode_literal(&Literal::new_simple_literal("a long string, hashed"))?,
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
            let result = std::panic::catch_unwind(|| QuadKey::parse(&prefix))
                .map_err(|_| TestError::new("parse never panics"))?;
            w.prefix_results.push((prefix, result));
        }
    }
    Ok(())
}

#[then("each parse fails with an error, and nothing panics")]
fn prefixes_fail(w: &mut Steps) {
    assert!(w.prefix_results.len() > 100);
    for (prefix, result) in &w.prefix_results {
        // A value-index key with a shorter token is itself a valid key: its token, the last field, may be empty.
        if let Ok(parsed) = result {
            assert!(
                matches!(parsed, (QuadKey::ValueIndex { token, .. }, None)
                    if prefix.len() >= 26 && b"abc".starts_with(token)),
                "prefix {prefix:?} parsed as {parsed:?}"
            );
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
fn map_regions(w: &mut Steps) -> TestResult {
    let (knows, name, a, c) = (11, 12, 21, 23);
    let key = |predicate, subject| QuadKey::Forward {
        namespace: 0,
        graph: DEFAULT_GRAPH,
        predicate,
        subject,
    };
    w.regions = [key(knows, a), key(knows, c), key(name, a)]
        .iter()
        .map(|k| Region::of(&k.to_bytes()))
        .collect::<Result<_, _>>()?;
    Ok(())
}

#[then("both map to the same region, and the forward key of name for a maps to another")]
fn regions_shared(w: &mut Steps) {
    assert_eq!(w.regions[0], w.regions[1]);
    assert_ne!(w.regions[0], w.regions[2]);
}

#[when("the key tests run")]
fn key_tests_run(w: &mut Steps) -> TestResult {
    w.golden_files = std::fs::read_dir(ultragraphdb_tool_bdd::testdata_dir()?.join("golden"))
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    Ok(())
}

#[then(
    "they check the quad layout against its specification and read no golden file generated from Dgraph"
)]
fn no_golden_files(w: &mut Steps) {
    assert!(w.golden_files.is_empty(), "{:?}", w.golden_files);
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
