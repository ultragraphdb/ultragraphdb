/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A list of the quad layout: its posting identity holds a literal's encoding beside the uid, its split keys are the
//! quad layout's, and every list operation keeps two literals whose fingerprints collide apart.

use std::ops::ControlFlow;

use ultragraphdb_core_codec::{Posting, PostingType};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey, key_prefix};
use ultragraphdb_store_posting::{
    DEL, List, PostingError, PostingErrorEnum, SET, is_quad_list, is_split_key, split_key_of,
};
use ultragraphdb_traits_store::{Store, StoreSnapshot};

use crate::utils::helpers::{new_lists, new_store, read_back, roll_up};

fn quad_key(subject: u64) -> Vec<u8> {
    QuadKey::Forward {
        namespace: 0,
        graph: DEFAULT_GRAPH,
        predicate: 7,
        subject,
    }
    .to_bytes()
}

/// A literal's posting under `fingerprint`, its value standing for the literal's encoding.
fn literal(fingerprint: u64, encoding: &[u8], op: u32) -> Posting {
    Posting::default()
        .with_uid(fingerprint)
        .with_value(encoding.to_vec())
        .with_posting_type(PostingType::Value)
        .with_op(op)
}

fn reference(uid: u64, op: u32) -> Posting {
    Posting::default().with_uid(uid).with_op(op)
}

/// Every posting at `read_ts`, as `(uid, value)`.
fn held(list: &List, read_ts: u64) -> Vec<(u64, Vec<u8>)> {
    let mut out = Vec::new();
    list.iterate(read_ts, 0, |p| {
        out.push((p.uid(), p.value().to_vec()));
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .unwrap();
    out
}

#[test]
fn test_quad_keys_are_lists_and_split_in_their_own_layout() {
    let base = quad_key(42);
    assert!(is_quad_list(&base));
    let part = split_key_of(&base, 1000).unwrap();
    assert_eq!(
        part,
        QuadKey::Forward {
            namespace: 0,
            graph: 0,
            predicate: 7,
            subject: 42
        }
        .split_bytes(1000)
        .unwrap()
    );
    assert!(is_split_key(&part).unwrap() && !is_split_key(&base).unwrap());
    assert!(is_quad_list(&part));
    assert!(!is_quad_list(
        &QuadKey::Statement {
            namespace: 0,
            graph: 0,
            statement: 1
        }
        .to_bytes()
    ));
    assert!(part.starts_with(&key_prefix(
        ultragraphdb_core_keys::Keyspace::Forward,
        true,
        0,
        Some(0),
        Some(7)
    )));
}

#[test]
fn test_colliding_literals_are_two_postings_in_the_scratch_layer_and_after_commit() {
    let mut list = List::empty(quad_key(1));
    list.set_ts(5);
    list.add_posting(literal(9, b"\x20\x01b", SET), false);
    list.add_posting(literal(9, b"\x20\x01a", SET), false);
    list.add_posting(reference(3, SET), false);
    list.add_posting(literal(9, b"\x20\x01a", SET), false);
    let expected = vec![
        (3, Vec::new()),
        (9, b"\x20\x01a".to_vec()),
        (9, b"\x20\x01b".to_vec()),
    ];
    assert_eq!(
        held(&list, 5),
        expected,
        "one posting per literal, ordered by uid then encoding"
    );
    list.commit_mutation(6);
    assert_eq!(held(&list, 6), expected);
    list.set_ts(7);
    list.add_posting(literal(9, b"\x20\x01a", DEL), false);
    assert_eq!(
        held(&list, 7),
        vec![(3, Vec::new()), (9, b"\x20\x01b".to_vec())],
        "deleting one keeps the other"
    );
}

#[test]
fn test_a_rollup_packs_a_shared_uid_once_and_reads_both_literals_back() {
    let store = new_store();
    let mut list = List::empty(quad_key(1));
    list.set_ts(5);
    for (fp, enc) in [(9u64, b"x".as_slice()), (9, b"w"), (4, b"v"), (12, b"u")] {
        list.add_posting(literal(fp, enc, SET), false);
    }
    list.commit_mutation(6);
    let (rolled, _) = roll_up(&store, &list, 6, usize::MAX);
    let pack_uids = ultragraphdb_core_codec::decode(rolled.plist().pack().unwrap(), 0);
    assert_eq!(
        pack_uids,
        vec![4, 9, 12],
        "the shared uid is one pack entry"
    );
    let expected = vec![
        (4, b"v".to_vec()),
        (9, b"w".to_vec()),
        (9, b"x".to_vec()),
        (12, b"u".to_vec()),
    ];
    assert_eq!(held(&rolled, u64::MAX), expected);
    let mut after = rolled.clone();
    after.set_ts(10);
    after.add_posting(literal(9, b"w", DEL), false);
    after.add_posting(literal(9, b"y", SET), false);
    assert_eq!(
        held(&after, 10),
        vec![
            (4, b"v".to_vec()),
            (9, b"x".to_vec()),
            (9, b"y".to_vec()),
            (12, b"u".to_vec())
        ],
        "a delta replaces and adds postings under a shared uid of the immutable layer"
    );
    assert_eq!(held(&read_back(&store, list.key()), u64::MAX), expected);
}

#[test]
fn test_a_requests_versionless_writes_commit_with_its_lists() {
    let lists = new_lists();
    let mut request = lists.request(lists.applied_ts());
    request
        .get_from_delta(&quad_key(1))
        .add_posting(reference(2, SET), false);
    request.add_versionless(b"term".to_vec(), b"uid".to_vec());
    request.add_versionless(b"term2".to_vec(), b"uid2".to_vec());
    assert_eq!(request.versionless().len(), 2);
    let mut batch = lists.batch();
    batch.push(request);
    let ts = lists.commit(batch).unwrap()[0];
    assert_eq!(
        lists.store().get_versionless(b"term").unwrap(),
        Some(b"uid".to_vec())
    );
    assert_eq!(
        lists.store().get_versionless(b"term2").unwrap(),
        Some(b"uid2".to_vec())
    );
    let snapshot = lists.store().snapshot(u64::MAX).unwrap();
    let item = snapshot
        .get(&quad_key(1))
        .unwrap()
        .expect("the list's delta");
    assert_eq!(
        (item.version(), item.meta()),
        (ts, ultragraphdb_store_posting::BIT_DELTA_POSTING)
    );
}

#[test]
fn test_a_split_key_is_the_flagged_keyspace_byte_and_the_start_appended() {
    // Hand-derived: Forward is 0x10 and the split flag 0x80; namespace, graph, predicate 7 and subject 42 as u64
    // big-endian; the start, 1000 = 0x03e8, appended big-endian.
    let mut expected = vec![0x90];
    expected.extend_from_slice(&[0; 8]);
    expected.extend_from_slice(&[0; 8]);
    expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 7]);
    expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 42]);
    expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x03, 0xe8]);
    assert_eq!(split_key_of(&quad_key(42), 1000).unwrap(), expected);
    let max = split_key_of(&quad_key(42), u64::MAX).unwrap();
    assert_eq!(&max[33..], &[0xff; 8], "the largest start");
    // The flag is set, not toggled: a key that carries it keeps it.
    assert_eq!(split_key_of(&max, 2).unwrap()[0], 0x90);
}

#[test]
fn test_every_list_keyspace_and_no_other_is_a_list() {
    // Hand-derived from the keyspace table: 0x10 to 0x16 hold lists; catalog 0x01, statements 0x17 and statistics
    // 0x18 do not, nor does a byte of no keyspace.
    for byte in 0x10..=0x16u8 {
        assert!(is_quad_list(&[byte]), "{byte:#x}");
        assert!(is_quad_list(&[byte | 0x80]), "{byte:#x} split");
        assert!(!is_split_key(&[byte]).unwrap());
        assert!(is_split_key(&[byte | 0x80]).unwrap());
    }
    for byte in [
        0x00, 0x01, 0x02, 0x0f, 0x17, 0x18, 0x19, 0x7f, 0x81, 0x97, 0xff,
    ] {
        assert!(!is_quad_list(&[byte]), "{byte:#x}");
    }
    assert!(!is_quad_list(&[]), "an empty key");
}

#[test]
fn test_a_key_of_no_list_has_no_split_key() {
    let statement = QuadKey::Statement {
        namespace: 0,
        graph: 0,
        statement: 1,
    }
    .to_bytes();
    for key in [statement.clone(), Vec::new(), b"key".to_vec()] {
        let not_a_list = PostingErrorEnum::NotAList { key: key.clone() };
        assert_eq!(split_key_of(&key, 1).unwrap_err().0, not_a_list);
        assert_eq!(is_split_key(&key).unwrap_err().0, not_a_list);
    }
    // A list read from a key of no list is refused as one, before any version is read.
    let error = List::read_versions(&statement, Vec::new()).unwrap_err();
    assert_eq!(error.0, PostingErrorEnum::NotAList { key: statement });
}

#[test]
fn test_a_vector_index_list_tells_postings_apart_by_uid_alone() {
    // A vector-index list holds one value per uid, which the next write replaces; a forward list keeps both literals.
    let vector = QuadKey::VectorIndex {
        namespace: 0,
        graph: 0,
        predicate: 7,
        list: 1,
        node: 3,
    }
    .to_bytes();
    let mut list = List::empty(vector);
    list.set_ts(5);
    list.add_posting(literal(9, b"a", SET), false);
    list.add_posting(literal(9, b"b", SET), false);
    assert_eq!(held(&list, 5), vec![(9, b"b".to_vec())]);
    let mut forward = List::empty(quad_key(1));
    forward.set_ts(5);
    forward.add_posting(literal(9, b"a", SET), false);
    forward.add_posting(literal(9, b"b", SET), false);
    assert_eq!(
        held(&forward, 5),
        vec![(9, b"a".to_vec()), (9, b"b".to_vec())]
    );
    // A reference posting is told apart by uid alone even in a forward list: its value carries nothing.
    let mut refs = List::empty(quad_key(2));
    refs.set_ts(5);
    refs.add_posting(reference(4, SET).with_value(b"x".to_vec()), false);
    refs.add_posting(reference(4, SET).with_value(b"y".to_vec()), false);
    assert_eq!(held(&refs, 5), vec![(4, b"y".to_vec())]);
}
