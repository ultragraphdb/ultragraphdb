/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A list of the quad layout: its posting identity holds a literal's encoding beside the uid, its split keys are the
//! quad layout's, and every list operation keeps two literals whose fingerprints collide apart.

use std::ops::ControlFlow;

use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey, key_prefix};
use ultragraphdb_proto_pb::pb::{Posting, posting};
use ultragraphdb_store_posting::{
    DEL, List, PostingError, SET, is_quad_list, is_split_key, split_key_of,
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
    Posting {
        uid: fingerprint,
        value: encoding.to_vec(),
        posting_type: posting::PostingType::Value as i32,
        op,
        ..Posting::default()
    }
}

fn reference(uid: u64, op: u32) -> Posting {
    Posting {
        uid,
        op,
        ..Posting::default()
    }
}

/// Every posting at `read_ts`, as `(uid, value)`.
fn held(list: &List, read_ts: u64) -> Vec<(u64, Vec<u8>)> {
    let mut out = Vec::new();
    list.iterate(read_ts, 0, |p| {
        out.push((p.uid, p.value.clone()));
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
    let pack_uids = ultragraphdb_core_codec::decode(rolled.plist().pack.as_ref().unwrap(), 0);
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
    let mut request = lists.request(lists.fresh_ts());
    request
        .get_from_delta(&quad_key(1))
        .add_posting(reference(2, SET), false);
    request.add_versionless(b"term".to_vec(), b"uid".to_vec());
    let mut batch = lists.batch();
    batch.push(request);
    lists.commit(batch).unwrap();
    assert_eq!(
        lists.store().get_versionless(b"term").unwrap(),
        Some(b"uid".to_vec())
    );
    let snapshot = lists.store().snapshot(u64::MAX).unwrap();
    assert!(snapshot.get(&quad_key(1)).unwrap().is_some());
}
