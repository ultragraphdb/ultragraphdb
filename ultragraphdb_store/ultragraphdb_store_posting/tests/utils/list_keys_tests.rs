/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A list of the quad layout: its posting identity holds a literal's encoding beside the uid, its split keys are the
//! quad layout's, and every list operation keeps two literals whose fingerprints collide apart.

use std::ops::ControlFlow;
use ultragraphdb_tool_bdd::{Context, TestResult};

use ultragraphdb_core_codec::{Posting, PostingType};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Graph, Keyspace, QuadKey, SplitStart, key_prefix};
use ultragraphdb_store_posting::{
    DEL, List, PostingError, PostingErrorEnum, SET, is_quad_list, is_split_key, split_key_of,
};
use ultragraphdb_traits_store::{Store, StoreSnapshot};

use crate::utils::helpers::{Failure, id, ids, new_lists, new_store, read_back, rid, roll_up};

fn forward(subject: u64) -> TestResult<QuadKey> {
    Ok(QuadKey::Forward {
        namespace: 0,
        graph: DEFAULT_GRAPH,
        predicate: id(7)?,
        subject: id(subject)?,
    })
}

fn quad_key(subject: u64) -> TestResult<Vec<u8>> {
    Ok(forward(subject)?.to_bytes())
}

/// A literal's posting under `fingerprint`, its value standing for the literal's encoding.
fn literal(fingerprint: u64, encoding: &[u8], op: u32) -> Posting {
    Posting::default()
        .with_uid(u128::from(fingerprint))
        .with_value(encoding.to_vec())
        .with_posting_type(PostingType::Value)
        .with_op(op)
}

/// A reference's posting to the ID numbered `n`.
fn reference(n: u64, op: u32) -> Posting {
    Posting::default().with_uid(rid(n)).with_op(op)
}

/// Every posting at `read_ts`, as `(key, value)`.
fn held(list: &List, read_ts: u64) -> TestResult<Vec<(u128, Vec<u8>)>> {
    let mut out = Vec::new();
    list.iterate(read_ts, 0, |p| {
        out.push((p.uid(), p.value().to_vec()));
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })?;
    Ok(out)
}

#[test]
fn test_quad_keys_are_lists_and_split_in_their_own_layout() -> TestResult {
    let base = quad_key(42)?;
    assert!(is_quad_list(&base));
    // A start below 2^64 is a value's fingerprint; any other a reference's ID.
    let part = split_key_of(&base, 1000)?;
    assert_eq!(
        part,
        forward(42)?.split_bytes(SplitStart::Fingerprint(1000))?
    );
    let id_part = split_key_of(&base, rid(1000))?;
    assert_eq!(
        id_part,
        forward(42)?.split_bytes(SplitStart::Id(id(1000)?))?
    );
    assert!(is_split_key(&part)? && !is_split_key(&base)?);
    assert!(is_quad_list(&part));
    assert!(!is_quad_list(
        &QuadKey::Statement {
            namespace: 0,
            graph: Graph::Default,
            statement: id(1)?
        }
        .to_bytes()
    ));
    assert!(part.starts_with(&key_prefix(
        Keyspace::Forward,
        true,
        0,
        Some(Graph::Default),
        Some(id(7)?)
    )));
    Ok(())
}

#[test]
fn test_a_split_start_that_is_no_id_or_a_fingerprint_on_a_value_index_key_is_refused() -> TestResult
{
    let corrupted = |result: Result<Vec<u8>, PostingError>| -> TestResult<String> {
        let error = result.failure("a split key")?;
        assert!(matches!(error.0, PostingErrorEnum::Store(_)), "{error:?}");
        Ok(error.to_string())
    };
    // 2^64 is no fingerprint and no valid ID: its type prefix is 0.
    let reason = corrupted(split_key_of(&quad_key(42)?, 1 << 64))?;
    assert!(
        reason.contains("a split start that is no valid ID"),
        "{reason}"
    );
    let value_index = QuadKey::ValueIndex {
        namespace: 0,
        graph: Graph::Default,
        predicate: id(7)?,
        tokenizer: 2,
        token: b"tok".to_vec(),
    }
    .to_bytes();
    let _reason: String = corrupted(split_key_of(&value_index, 1000))?;
    assert!(is_split_key(&split_key_of(&value_index, rid(5))?)?);
    Ok(())
}

#[test]
fn test_colliding_literals_are_two_postings_in_the_scratch_layer_and_after_commit() -> TestResult {
    let mut list = List::empty(quad_key(1)?);
    list.set_ts(5);
    list.add_posting(literal(9, b"\x20\x01b", SET), false);
    list.add_posting(literal(9, b"\x20\x01a", SET), false);
    list.add_posting(reference(3, SET), false);
    list.add_posting(literal(9, b"\x20\x01a", SET), false);
    // A value's fingerprint sorts below every reference's ID.
    let expected = vec![
        (9, b"\x20\x01a".to_vec()),
        (9, b"\x20\x01b".to_vec()),
        (rid(3), Vec::new()),
    ];
    assert_eq!(
        held(&list, 5)?,
        expected,
        "one posting per literal, ordered by key then encoding"
    );
    list.commit_mutation(6);
    assert_eq!(held(&list, 6)?, expected);
    list.set_ts(7);
    list.add_posting(literal(9, b"\x20\x01a", DEL), false);
    assert_eq!(
        held(&list, 7)?,
        vec![(9, b"\x20\x01b".to_vec()), (rid(3), Vec::new())],
        "deleting one keeps the other"
    );
    Ok(())
}

#[test]
fn test_a_rollup_packs_references_alone_and_reads_both_literals_back() -> TestResult {
    let store = new_store();
    let mut list = List::empty(quad_key(1)?);
    list.set_ts(5);
    for (fp, enc) in [(9u64, b"x".as_slice()), (9, b"w"), (4, b"v"), (12, b"u")] {
        list.add_posting(literal(fp, enc, SET), false);
    }
    list.add_posting(reference(5, SET), false);
    list.commit_mutation(6);
    let (rolled, _) = roll_up(&store, &list, 6, usize::MAX)?;
    let pack_ids = ultragraphdb_core_codec::decode(rolled.plist().pack().context("a pack")?, 0);
    assert_eq!(
        pack_ids,
        ids(&[5])?,
        "a pack holds valid IDs alone, never a fingerprint"
    );
    let expected = vec![
        (4, b"v".to_vec()),
        (9, b"w".to_vec()),
        (9, b"x".to_vec()),
        (12, b"u".to_vec()),
        (rid(5), Vec::new()),
    ];
    assert_eq!(held(&rolled, u64::MAX)?, expected);
    let mut after = rolled.clone();
    after.set_ts(10);
    after.add_posting(literal(9, b"w", DEL), false);
    after.add_posting(literal(9, b"y", SET), false);
    assert_eq!(
        held(&after, 10)?,
        vec![
            (4, b"v".to_vec()),
            (9, b"x".to_vec()),
            (9, b"y".to_vec()),
            (12, b"u".to_vec()),
            (rid(5), Vec::new()),
        ],
        "a delta replaces and adds postings under a shared fingerprint of the immutable layer"
    );
    assert_eq!(held(&read_back(&store, list.key())?, u64::MAX)?, expected);
    // A list of literals alone rolls up with no ID to pack.
    let mut literals = List::empty(quad_key(2)?);
    literals.set_ts(5);
    literals.add_posting(literal(4, b"v", SET), false);
    literals.commit_mutation(6);
    let (rolled_literals, _) = roll_up(&store, &literals, 6, usize::MAX)?;
    assert!(rolled_literals.plist().pack().is_none());
    assert_eq!(held(&rolled_literals, u64::MAX)?, vec![(4, b"v".to_vec())]);
    Ok(())
}

#[test]
fn test_a_requests_versionless_writes_commit_with_its_lists() -> TestResult {
    let lists = new_lists();
    let mut request = lists.request(lists.applied_ts());
    request
        .get_from_delta(&quad_key(1)?)
        .add_posting(reference(2, SET), false);
    request.add_versionless(b"term".to_vec(), b"uid".to_vec());
    request.add_versionless(b"term2".to_vec(), b"uid2".to_vec());
    assert_eq!(request.versionless().len(), 2);
    let mut batch = lists.batch();
    batch.push(request);
    let ts = lists.commit(batch)?[0];
    assert_eq!(
        lists.store().get_versionless(b"term")?,
        Some(b"uid".to_vec())
    );
    assert_eq!(
        lists.store().get_versionless(b"term2")?,
        Some(b"uid2".to_vec())
    );
    let snapshot = lists.store().snapshot(u64::MAX)?;
    let item = snapshot.get(&quad_key(1)?)?.context("the list's delta")?;
    assert_eq!(
        (item.version(), item.meta()),
        (ts, ultragraphdb_store_posting::BIT_DELTA_POSTING)
    );
    Ok(())
}

#[test]
fn test_a_split_key_is_the_flagged_keyspace_byte_the_kind_and_the_start_appended() -> TestResult {
    // Written from the key layout: Forward is 0x10 and the split flag 0x80; the namespace as u64 big-endian, the
    // default graph's 16 zero bytes, predicate 7 and subject 42 as IDs of type 1 and bucket 0, 16 big-endian bytes
    // each; then kind 1 and the fingerprint 1000 = 0x03e8 as 8 big-endian bytes, or kind 2 and an ID's 16 bytes.
    let id_bytes = |sequence: u8| [0, 1, 0, 0, 0, 0, 0, 0, 0, sequence, 0, 0, 0, 0, 0, 0];
    let mut expected = vec![0x90];
    expected.extend_from_slice(&[0; 8]);
    expected.extend_from_slice(&[0; 16]);
    expected.extend_from_slice(&id_bytes(7));
    expected.extend_from_slice(&id_bytes(42));
    let base_len = expected.len();
    expected.push(1);
    expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x03, 0xe8]);
    assert_eq!(split_key_of(&quad_key(42)?, 1000)?, expected);
    expected.truncate(base_len);
    expected.push(2);
    expected.extend_from_slice(&id_bytes(9));
    assert_eq!(split_key_of(&quad_key(42)?, rid(9))?, expected);
    let max = split_key_of(&quad_key(42)?, u128::from(u64::MAX))?;
    assert_eq!(
        max.get(base_len + 1..),
        Some([0xff; 8].as_slice()),
        "the largest fingerprint"
    );
    // The flag is set, not toggled: a key that carries it keeps it.
    assert_eq!(split_key_of(&quad_key(42)?, 2)?.first(), Some(&0x90));
    Ok(())
}

#[test]
fn test_every_list_keyspace_and_no_other_is_a_list() -> TestResult {
    // Hand-derived from the keyspace table: 0x10 to 0x16 hold lists; catalog 0x01, statements 0x17, statistics 0x18,
    // marks 0x19, referrers 0x1A and the dictionary 0x1B do not, nor does a byte of no keyspace.
    for byte in 0x10..=0x16u8 {
        assert!(is_quad_list(&[byte]), "{byte:#x}");
        assert!(is_quad_list(&[byte | 0x80]), "{byte:#x} split");
        assert!(!is_split_key(&[byte])?);
        assert!(is_split_key(&[byte | 0x80])?);
    }
    for byte in [
        0x00, 0x01, 0x02, 0x0f, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x7f, 0x81, 0x97, 0xff,
    ] {
        assert!(!is_quad_list(&[byte]), "{byte:#x}");
    }
    assert!(!is_quad_list(&[]), "an empty key");
    Ok(())
}

#[test]
fn test_a_key_of_no_list_has_no_split_key() -> TestResult {
    let statement = QuadKey::Statement {
        namespace: 0,
        graph: Graph::Default,
        statement: id(1)?,
    }
    .to_bytes();
    for key in [statement.clone(), Vec::new(), b"key".to_vec()] {
        let not_a_list = PostingErrorEnum::NotAList { key: key.clone() };
        assert_eq!(
            split_key_of(&key, rid(1))
                .failure("a split key of no list")?
                .0,
            not_a_list
        );
        assert_eq!(
            is_split_key(&key).failure("the split test of no list")?.0,
            not_a_list
        );
    }
    // A list read from a key of no list is refused as one, before any version is read.
    let error = List::read_versions(&statement, Vec::new()).failure("a read of no list")?;
    assert_eq!(error.0, PostingErrorEnum::NotAList { key: statement });
    Ok(())
}

#[test]
fn test_a_vector_index_list_tells_postings_apart_by_uid_alone() -> TestResult {
    // A vector-index list holds one value per uid, which the next write replaces; a forward list keeps both literals.
    let vector = QuadKey::VectorIndex {
        namespace: 0,
        graph: Graph::Default,
        predicate: id(7)?,
        list: 1,
        node: id(3)?,
    }
    .to_bytes();
    let mut list = List::empty(vector);
    list.set_ts(5);
    list.add_posting(literal(9, b"a", SET), false);
    list.add_posting(literal(9, b"b", SET), false);
    assert_eq!(held(&list, 5)?, vec![(9, b"b".to_vec())]);
    let mut forward_list = List::empty(quad_key(1)?);
    forward_list.set_ts(5);
    forward_list.add_posting(literal(9, b"a", SET), false);
    forward_list.add_posting(literal(9, b"b", SET), false);
    assert_eq!(
        held(&forward_list, 5)?,
        vec![(9, b"a".to_vec()), (9, b"b".to_vec())]
    );
    // A reference posting is told apart by uid alone even in a forward list: its value carries nothing.
    let mut refs = List::empty(quad_key(2)?);
    refs.set_ts(5);
    refs.add_posting(reference(4, SET).with_value(b"x".to_vec()), false);
    refs.add_posting(reference(4, SET).with_value(b"y".to_vec()), false);
    assert_eq!(held(&refs, 5)?, vec![(rid(4), b"y".to_vec())]);
    Ok(())
}
