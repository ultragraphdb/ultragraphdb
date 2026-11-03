/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of the quad keys, bound to `features/`: their encoding, parse and refusals, their order, and their
//! regions.

use cucumber::{World, then, when};
use oxrdf::Literal;
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{
    DEFAULT_GRAPH, Graph, Keyspace, QuadKey, QuadKeyError, Region, SplitStart, key_prefix,
};
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// A quad key's parse: its fields and a split part's start, or why it was refused.
type QuadParse = Result<(QuadKey, Option<SplitStart>), QuadKeyError>;

#[derive(Debug, Default, World)]
struct Steps {
    key: Vec<u8>,
    golden_files: Vec<std::path::PathBuf>,
    quad: Option<QuadParse>,
    quad_keys: Vec<Vec<u8>>,
    ids: Vec<SystemId>,
    regions: Vec<Region>,
    prefix_results: Vec<(Vec<u8>, QuadParse)>,
    scanned: Vec<Vec<u8>>,
}

fn id(prefix: u16, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::new(prefix)?,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

/// The scenario's forward key: namespace 133, the default graph, predicate (1, 7, 7), the subject given.
fn forward(subject: SystemId) -> TestResult<QuadKey> {
    Ok(QuadKey::Forward {
        namespace: 133,
        graph: DEFAULT_GRAPH,
        predicate: id(1, 7, 7)?,
        subject,
    })
}

fn encode_literal(literal: &Literal) -> TestResult<EncodedTerm> {
    Ok(EncodedTerm::literal(literal.as_ref(), |_| {
        Err::<SystemId, _>(TermError::malformed(Vec::new(), "no datatype ID is needed"))
    })
    .context("encodes")?
    .into_term())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[when(
    "a forward key for namespace 133, the default graph, the predicate with ID (type 1, bucket 7, sequence 7) and the subject with ID (type 1, bucket 42, sequence 42) is parsed"
)]
fn parse_forward(w: &mut Steps) -> TestResult {
    w.quad = Some(QuadKey::parse(&forward(id(1, 42, 42)?)?.to_bytes()));
    Ok(())
}

#[then(
    "the result is a forward key with namespace 133, the default graph, and those predicate and subject IDs"
)]
fn forward_parsed(w: &mut Steps) -> TestResult {
    let want = QuadKey::Forward {
        namespace: 133,
        graph: Graph::Default,
        predicate: id(1, 7, 7)?,
        subject: id(1, 42, 42)?,
    };
    assert_eq!(w.quad.take(), Some(Ok((want, None))));
    Ok(())
}

#[when("a forward key carrying a subject ID whose sequence is 0 is parsed")]
fn parse_zero_subject(w: &mut Steps) -> TestResult {
    let mut key = forward(id(1, 42, 42)?)?.to_bytes();
    // The subject's sequence: after the keyspace byte, the namespace (8), the graph (16), the predicate (16) and the
    // subject's type prefix (2), at bytes 43 to 50.
    key.get_mut(43..51)
        .context("the subject's sequence")?
        .fill(0);
    w.quad = Some(QuadKey::parse(&key));
    w.key = key;
    Ok(())
}

#[then("parsing fails with an error naming the key's bytes")]
fn fails_naming_key(w: &mut Steps) -> TestResult {
    let Err(error) = w.quad.take().context("parsed")? else {
        return Err(TestError::new("refused"));
    };
    assert!(error.to_string().contains(&hex(&w.key)), "{error}");
    Ok(())
}

#[when("a reverse key whose object is the encoding of `\"Ann\"@en` is built and parsed")]
fn parse_reverse_literal(w: &mut Steps) -> TestResult {
    let key = QuadKey::Reverse {
        namespace: 0,
        graph: DEFAULT_GRAPH,
        predicate: id(1, 9, 9)?,
        object: encode_literal(&Literal::new_language_tagged_literal("Ann", "en")?)?,
    };
    w.key = key.to_bytes();
    w.quad = Some(QuadKey::parse(&w.key));
    Ok(())
}

#[then("the parsed object decodes to `\"Ann\"@en`, and the key ends with the object's encoding")]
fn reverse_object_decodes(w: &mut Steps) -> TestResult {
    let quad = w.quad.take();
    let Some(Ok((QuadKey::Reverse { object, .. }, None))) = &quad else {
        return Err(TestError::new(format!("a reverse key, not {quad:?}")));
    };
    // The literal is inline: neither lookup may be asked.
    let literal = object.view()?.to_literal(
        |_| Err(TermError::malformed(Vec::new(), "no UID lookup")),
        |_| Err(TermError::malformed(Vec::new(), "no hash lookup")),
    )?;
    assert_eq!(literal, Literal::new_language_tagged_literal("Ann", "en")?);
    assert!(w.key.ends_with(object.as_bytes()));
    Ok(())
}

#[when(
    "a mark key for namespace 7, the entity with ID (type 1, bucket 42, sequence 9) and bucket group 0x2_ABCD is built and parsed, and the object-predicates keys of the graph with that same ID are scanned by prefix"
)]
fn parse_mark(w: &mut Steps) -> TestResult {
    let entity = id(1, 42, 9)?;
    let key = QuadKey::Mark {
        namespace: 7,
        entity,
        group: 0x2_ABCD,
    };
    w.key = key.to_bytes();
    w.quad = Some(QuadKey::parse(&w.key));
    let prefix = key_prefix(
        Keyspace::ObjectPredicates,
        false,
        7,
        Some(Graph::Named(entity)),
        None,
    );
    // The scan over every key the scenario holds: the mark key alone.
    w.scanned = [w.key.clone()]
        .into_iter()
        .filter(|k| k.starts_with(&prefix))
        .collect();
    w.ids.push(entity);
    Ok(())
}

#[then(
    "the parse returns the marks keyspace, namespace 7, that ID and group 0x2_ABCD from the bytes `02 AB CD`, and the prefix scan returns no mark key"
)]
fn mark_parsed(w: &mut Steps) -> TestResult {
    let entity = *w.ids.first().context("the entity")?;
    let (parsed, start) = w.quad.take().context("parsed")??;
    assert_eq!(parsed.keyspace(), Keyspace::Marks);
    assert_eq!(
        parsed,
        QuadKey::Mark {
            namespace: 7,
            entity,
            group: 0x2_ABCD
        }
    );
    assert_eq!(start, None);
    assert!(w.key.ends_with(&[0x02, 0xAB, 0xCD]));
    assert!(w.scanned.is_empty(), "{:?}", w.scanned);
    Ok(())
}

#[when(
    "a referrer key for namespace 7 and the entity with ID (type 1, bucket 42, sequence 9) is built and parsed"
)]
fn parse_referrers(w: &mut Steps) -> TestResult {
    let entity = id(1, 42, 9)?;
    w.quad = Some(QuadKey::parse(
        &QuadKey::Referrers {
            namespace: 7,
            entity,
        }
        .to_bytes(),
    ));
    w.ids.push(entity);
    Ok(())
}

#[then("the parse returns the referrers keyspace, namespace 7 and that entity's ID")]
fn referrers_parsed(w: &mut Steps) -> TestResult {
    let entity = *w.ids.first().context("the entity")?;
    let (parsed, start) = w.quad.take().context("parsed")??;
    assert_eq!(parsed.keyspace(), Keyspace::Referrers);
    assert_eq!(parsed.namespace(), 7);
    assert_eq!(
        parsed,
        QuadKey::Referrers {
            namespace: 7,
            entity
        }
    );
    assert_eq!(start, None);
    Ok(())
}

#[when(
    "the split key for a forward key and the start ID (type 1, bucket 5, sequence 1000) is built and parsed"
)]
fn parse_split_forward(w: &mut Steps) -> TestResult {
    let start = SplitStart::Id(id(1, 5, 1000)?);
    w.quad = Some(QuadKey::parse(
        &forward(id(1, 42, 42)?)?.split_bytes(start)?,
    ));
    Ok(())
}

#[then("the parse returns the base key's parts and that start ID")]
fn split_parsed(w: &mut Steps) -> TestResult {
    let want = (
        forward(id(1, 42, 42)?)?,
        Some(SplitStart::Id(id(1, 5, 1000)?)),
    );
    assert_eq!(w.quad.take(), Some(Ok(want)));
    Ok(())
}

#[when(
    "the split key of a forward key at the start ID (type 1, bucket 5, sequence 1000) loses its last 8 bytes and is parsed"
)]
fn parse_split_id_start_cut(w: &mut Steps) -> TestResult {
    let key = forward(id(1, 42, 42)?)?.split_bytes(SplitStart::Id(id(1, 5, 1000)?))?;
    let cut = key
        .len()
        .checked_sub(8)
        .context("a split key longer than 8 bytes")?;
    let key = key.get(..cut).context("the cut key")?.to_vec();
    w.quad = Some(QuadKey::parse(&key));
    w.key = key;
    Ok(())
}

#[when("one byte is appended to a valid split forward key and the result is parsed")]
fn parse_quad_split_trailing(w: &mut Steps) -> TestResult {
    let mut key = forward(id(1, 42, 42)?)?.split_bytes(SplitStart::Id(id(1, 5, 1000)?))?;
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
        forward(id(1, 42, 42)?)?,
        QuadKey::Reverse {
            namespace: 1,
            graph: Graph::Named(id(1, 3, 3)?),
            predicate: id(1, 9, 9)?,
            object: encode_literal(&Literal::new_simple_literal("a long string, hashed"))?,
        },
        QuadKey::ValueIndex {
            namespace: 1,
            graph: Graph::Named(id(1, 3, 3)?),
            predicate: id(1, 9, 9)?,
            tokenizer: 2,
            token: b"abc".to_vec(),
        },
        QuadKey::Statement {
            namespace: 1,
            graph: Graph::Named(id(1, 3, 3)?),
            statement: id(2, 3, 77)?,
        },
    ];
    for key in keys {
        let bytes = key.to_bytes();
        for end in 0..bytes.len() {
            let prefix = bytes.get(..end).context("a prefix")?.to_vec();
            let result = std::panic::catch_unwind(|| QuadKey::parse(&prefix))
                .map_err(|_| TestError::new("parse never panics"))?;
            w.prefix_results.push((prefix, result));
        }
    }
    Ok(())
}

#[then("each parse fails with an error, and nothing panics")]
fn prefixes_fail(w: &mut Steps) {
    // At least 57 + 41 + 45 + 41 prefixes: the forward key's 57 bytes, the reverse key's 41 before its object, the
    // value-index key's 45 and the statement key's 41.
    assert!(w.prefix_results.len() > 184, "{}", w.prefix_results.len());
    for (prefix, result) in &w.prefix_results {
        // A value-index key with a shorter token is itself a valid key: its token, the last field, may be empty.
        if let Ok(parsed) = result {
            assert!(
                matches!(parsed, (QuadKey::ValueIndex { token, .. }, None)
                    if prefix.len() >= 42 && b"abc".starts_with(token)),
                "prefix {prefix:?} parsed as {parsed:?}"
            );
        }
    }
}

#[when(
    "a forward key is cut inside its subject ID, at each of the ID's 15 inner positions, and parsed"
)]
fn parse_cut_subject(w: &mut Steps) -> TestResult {
    let bytes = forward(id(1, 42, 42)?)?.to_bytes();
    // The subject is the last 16 bytes, from byte 41: cut after 1 to 15 of them.
    for inner in 1..16 {
        let prefix = bytes.get(..41 + inner).context("a cut")?.to_vec();
        let result = std::panic::catch_unwind(|| QuadKey::parse(&prefix))
            .map_err(|_| TestError::new("parse never panics"))?;
        w.prefix_results.push((prefix, result));
    }
    Ok(())
}

#[then("each parse fails with an error naming the key's bytes")]
fn cuts_fail_naming_key(w: &mut Steps) -> TestResult {
    assert_eq!(w.prefix_results.len(), 15);
    for (prefix, result) in &w.prefix_results {
        let Err(error) = result else {
            return Err(TestError::new(format!("{prefix:?} parsed as {result:?}")));
        };
        assert!(error.to_string().contains(&hex(prefix)), "{error}");
    }
    Ok(())
}

#[when(
    "forward keys of one predicate are built for subjects of one type and one bucket with sequences 1, 2, 256 and 2^40"
)]
fn build_forward_keys(w: &mut Steps) -> TestResult {
    for sequence in [1u64, 2, 256, 1 << 40] {
        let subject = id(1, 0xAB_CDEF, sequence)?;
        w.quad_keys.push(forward(subject)?.to_bytes());
        w.ids.push(subject);
    }
    Ok(())
}

#[then("their byte order equals the subjects' IDs' numeric order")]
fn forward_keys_sorted(w: &mut Steps) {
    let mut by_bytes: Vec<(Vec<u8>, SystemId)> = w
        .quad_keys
        .iter()
        .cloned()
        .zip(w.ids.iter().copied())
        .collect();
    by_bytes.sort();
    let mut by_number = w.ids.clone();
    by_number.sort_by_key(|id| id.to_u128());
    assert_eq!(
        by_bytes.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        by_number
    );
    assert!(
        w.quad_keys.is_sorted(),
        "built in numeric order, so already sorted"
    );
}

#[when(
    "the forward keys of `knows` for `a` and for `c` in the default graph are mapped to regions"
)]
fn map_regions(w: &mut Steps) -> TestResult {
    let (knows, name, a, c) = (id(1, 1, 11)?, id(1, 2, 12)?, id(1, 3, 21)?, id(1, 3, 23)?);
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

#[then("both map to the same region, and the forward key of `name` for `a` maps to another")]
fn regions_shared(w: &mut Steps) -> TestResult {
    let [knows_a, knows_c, name_a] = w.regions.as_slice() else {
        return Err(TestError::new("three regions"));
    };
    assert_eq!(knows_a, knows_c);
    assert_ne!(knows_a, name_a);
    Ok(())
}

#[when("the referrer keys of two entities of namespace 7 are mapped to regions")]
fn map_referrer_regions(w: &mut Steps) -> TestResult {
    let referrers = |namespace, entity| QuadKey::Referrers { namespace, entity };
    w.regions = [
        referrers(7, id(1, 42, 9)?),
        referrers(7, id(3, 1, 10)?),
        referrers(8, id(1, 42, 9)?),
    ]
    .iter()
    .map(|k| Region::of(&k.to_bytes()))
    .collect::<Result<_, _>>()?;
    Ok(())
}

#[then(
    "both map to the referrers region of namespace 7, and a referrer key of namespace 8 maps to another"
)]
fn referrer_regions_shared(w: &mut Steps) -> TestResult {
    let [first, second, other] = w.regions.as_slice() else {
        return Err(TestError::new("three regions"));
    };
    assert_eq!(first, second);
    assert_eq!(
        (
            first.keyspace(),
            first.namespace(),
            first.graph(),
            first.predicate()
        ),
        (Keyspace::Referrers, 7, None, None)
    );
    assert_ne!(first, other);
    assert_eq!(other.namespace(), 8);
    Ok(())
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
