/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list_test.go. Changed for ultragraphdb: translated to Rust; a list's
 * transactions are a read timestamp set on it and an in-place commit, as the Go tests drive a list by hand; Badger
 * is the simulated store; the split threshold is a parameter of the rollup where Go changes a package variable;
 * TestLargePlistSplit's values are a hundredth of Go's, against a split threshold of Dgraph's own size.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::ops::ControlFlow;

use ultragraphdb_common_utils::go_farm::fingerprint64;
use ultragraphdb_core_codec::decode;
use ultragraphdb_proto_pb::pb::{Posting, PostingList, posting};
use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING, List, ListOptions, MAX_LIST_SIZE, PostingError,
    bin_split, from_backup_posting_list, marshal, split_key_of, unmarshal_complete,
    unmarshal_delta,
};
use ultragraphdb_store_sim::SimStore;

use crate::utils::helpers::{
    add, check_uids, del_uid, del_value, delete_all, key, new_store, read_back, roll_up, set_uid,
    set_value, uids, with_facet, write_entries,
};

/// A vector-index list of `attr` for `node`: a list holding one value, which the next write replaces, as the vector
/// index's lists hold theirs.
fn single_key(attr: &str, node: u64) -> Vec<u8> {
    ultragraphdb_core_keys::QuadKey::VectorIndex {
        namespace: 0,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: crate::utils::helpers::predicate(attr),
        list: 1,
        node,
    }
    .to_bytes()
}

/// A set of the one value of a single-value list.
fn set_single(value: &str) -> Posting {
    Posting {
        uid: u64::MAX,
        ..set_value(value)
    }
}

/// A delete of the one value of a single-value list.
fn del_single(value: &str) -> Posting {
    Posting {
        uid: u64::MAX,
        ..del_value(value)
    }
}

fn first(list: &List, read_ts: u64) -> Posting {
    let mut out = Posting::default();
    list.iterate(read_ts, 0, |p| {
        out = p.clone();
        Ok::<_, PostingError>(ControlFlow::Break(()))
    })
    .expect("iterate");
    out
}

fn check_value(list: &List, value: &str, read_ts: u64) {
    let p = first(list, read_ts);
    assert_eq!(p.uid, u64::MAX);
    assert_eq!(p.value, value.as_bytes());
}

fn count(list: &List, read_ts: u64) -> usize {
    uids(list, read_ts, 0).len()
}

#[test]
fn test_get_single_posting() {
    let k = key("GetSinglePosting", 123);
    let create = |start_ts: u64, commit_ts: u64| PostingList {
        postings: vec![Posting {
            uid: 1,
            op: 1,
            start_ts,
            commit_ts,
            ..Posting::default()
        }],
        commit_ts: start_ts,
        ..PostingList::default()
    };
    let l = List::empty(k.clone());
    assert!(l.static_value(1).is_none());

    let l = List::new(k, create(1, 1), 0);
    assert_eq!(l.static_value(1).unwrap().postings[0].start_ts, 1);
    assert_eq!(l.static_value(4).unwrap().postings[0].start_ts, 1);

    let l = l.committed(3, create(2, 3));
    assert_eq!(l.static_value(1).unwrap().postings[0].start_ts, 1);
    assert_eq!(l.static_value(3).unwrap().postings[0].start_ts, 2);
    assert_eq!(l.static_value(4).unwrap().postings[0].start_ts, 2);

    // A transaction from 4 to 6.
    let l = l.committed(6, create(4, 6));
    assert_eq!(l.static_value(5).unwrap().postings[0].start_ts, 2);
    assert_eq!(l.static_value(6).unwrap().postings[0].start_ts, 4);
}

#[test]
fn test_add_mutation() {
    let mut l = List::empty(key("name", 2));
    let edge = with_facet(set_uid(9), "testing");
    add(&mut l, 1, &edge);
    assert_eq!(uids(&l, 1, 0), vec![9]);
    assert_eq!(first(&l, 1).facets[0].key, "testing");

    add(&mut l, 1, &with_facet(set_uid(81), "testing"));
    assert_eq!(uids(&l, 1, 0), vec![9, 81]);
    add(&mut l, 1, &with_facet(set_uid(49), "testing"));
    assert_eq!(uids(&l, 1, 0), vec![9, 49, 81]);
    check_uids(&l, &[9, 49, 81], 1);

    // Delete an edge, add an edge, replace an edge.
    add(&mut l, 1, &del_uid(49));
    add(&mut l, 1, &with_facet(set_uid(69), "testing"));
    add(&mut l, 1, &with_facet(set_uid(9), "anti-testing"));
    l.commit_mutation(2);

    check_uids(&l, &[9, 69, 81], 3);
    assert_eq!(first(&l, 3).facets[0].key, "anti-testing");
}

#[test]
fn test_add_mutation_value() {
    let store = new_store();
    let mut ol = read_back(&store, &single_key("value", 10));
    add(&mut ol, 1, &set_single("oh hey there"));
    check_value(&ol, "oh hey there", 1);

    // The same check after committing.
    ol.commit_mutation(2);
    check_value(&ol, "oh hey there", 3);

    // The value made it to the posting list. Changing it now.
    add(&mut ol, 3, &set_single("119"));
    check_value(&ol, "119", 3);
}

#[test]
fn test_add_mutation_jchiu1() {
    let mut ol = List::empty(single_key("value", 12));
    add(&mut ol, 1, &set_single("cars"));
    ol.commit_mutation(2);
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "cars", 3);

    // Set the value to newcars, but do not commit yet.
    add(&mut ol, 3, &set_single("newcars"));
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "newcars", 3);

    add(&mut ol, 3, &set_single("someothercars"));
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "someothercars", 3);

    // Back to the committed value.
    add(&mut ol, 3, &set_single("cars"));
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "cars", 3);
}

#[test]
fn test_add_mutation_del_set() {
    let k = single_key("value", 1534);
    // A delete-all, not committed, on one copy of the list.
    let mut ol = List::empty(k.clone());
    add(&mut ol, 1, &delete_all());

    // Another copy sets newcars and commits it.
    let mut ol1 = List::empty(k);
    add(&mut ol1, 2, &set_single("newcars"));
    ol1.commit_mutation(3);
    assert_eq!(ol1.length(3, 0), 1);
    check_value(&ol1, "newcars", 3);
}

#[test]
fn test_add_mutation_del_read() {
    let mut ol = List::empty(single_key("value", 1543));
    add(&mut ol, 1, &set_single("newcars"));
    ol.commit_mutation(2);
    assert_eq!(ol.length(2, 0), 1);
    check_value(&ol, "newcars", 2);

    // A delete-all, not committed.
    add(&mut ol, 3, &delete_all());
    // Its own transaction sees zero length.
    assert_eq!(ol.length(3, 0), 0);

    ol.commit_mutation(5);
    // A read at 4 does not see the delete-all committed at 5.
    assert_eq!(ol.length(4, 0), 1);
    check_value(&ol, "newcars", 4);
    assert_eq!(ol.length(6, 0), 0);
}

#[test]
fn test_add_mutation_jchiu2() {
    let mut ol = List::empty(single_key("value", 15));
    add(&mut ol, 1, &del_single("cars"));
    assert_eq!(ol.length(1, 0), 0);

    add(&mut ol, 1, &set_single("newcars"));
    assert_eq!(ol.length(1, 0), 1);
    check_value(&ol, "newcars", 1);
}

#[test]
fn test_add_mutation_jchiu2_commit() {
    let mut ol = List::empty(single_key("value", 16));
    add(&mut ol, 1, &del_single("cars"));
    ol.commit_mutation(2);
    assert_eq!(ol.length(3, 0), 0);

    add(&mut ol, 3, &set_single("newcars"));
    ol.commit_mutation(4);
    assert_eq!(ol.length(5, 0), 1);
    check_value(&ol, "newcars", 5);
}

#[test]
fn test_add_mutation_jchiu3() {
    let mut ol = List::empty(single_key("value", 29));
    add(&mut ol, 1, &set_single("cars"));
    ol.commit_mutation(2);
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "cars", 3);

    add(&mut ol, 3, &del_single("cars"));
    assert_eq!(ol.length(3, 0), 0);

    add(&mut ol, 3, &set_single("newcars"));
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "newcars", 3);

    add(&mut ol, 3, &del_single("newcars"));
    assert_eq!(ol.length(3, 0), 0);
}

#[test]
fn test_add_mutation_mrjn1() {
    let mut ol = List::empty(single_key("value", 21));
    add(&mut ol, 1, &set_single("cars"));
    ol.commit_mutation(2);

    // Delete the committed value, twice, without committing.
    add(&mut ol, 3, &del_single("cars"));
    assert_eq!(ol.length(3, 0), 0);
    add(&mut ol, 3, &del_single("cars"));
    assert_eq!(ol.length(3, 0), 0);

    // Set it again.
    add(&mut ol, 3, &set_single("cars"));
    check_value(&ol, "cars", 3);

    // And delete it again.
    add(&mut ol, 3, &del_single("cars"));
    assert_eq!(ol.length(3, 0), 0);
}

/// The untagged value of the newest version of `key` at or below `read_ts`, by a point get: what Go's
/// `GetSinglePosting` reads from Badger.
fn stored_value(store: &SimStore, key: &[u8], read_ts: u64) -> Vec<u8> {
    let item = ultragraphdb_traits_store::Store::get_at(store, key, read_ts)
        .expect("get")
        .expect("a version");
    let plist = if item.meta() == BIT_COMPLETE_POSTING {
        unmarshal_complete(key, item.value()).expect("complete")
    } else {
        unmarshal_delta(key, item.value()).expect("delta")
    };
    plist
        .postings
        .iter()
        .find(|p| p.uid == u64::MAX)
        .map(|p| p.value.clone())
        .unwrap_or_default()
}

#[test]
fn test_read_single_value() {
    let store = new_store();
    let k = single_key("value", 1240);
    let mut ol = read_back(&store, &k);
    let n = 10_000u64;
    for i in (2..=n).step_by(2) {
        add(&mut ol, i, &set_single(&format!("ho hey there{i}")));
        let delta = ol.delta().cloned().expect("a delta");
        ol.commit_mutation(i + 1);
        write_entries(
            &store,
            vec![ultragraphdb_traits_store::Entry::put(
                k.clone(),
                i + 1,
                ultragraphdb_store_posting::BIT_DELTA_POSTING,
                marshal(&delta),
            )],
        );

        if i % 10 == 0 {
            // Frequent rollups, stored at an old timestamp.
            ol = roll_up(&store, &ol, i - 3, usize::MAX).0;
        }

        for j in 3.max(ol.min_ts())..i + 6 {
            let value = stored_value(&store, &k, j);
            check_value(&ol, std::str::from_utf8(&value).expect("utf-8"), j);
        }
    }
}

#[test]
fn test_rollup_max_ts_is_set() {
    let store = new_store();
    let k = key("bal", 1333);
    let mut ol = read_back(&store, &k);
    let n = 1_000_000u64;
    for i in (2..=n).step_by(2) {
        add(&mut ol, i, &set_uid(i));
        ol.commit_mutation(i + 1);
        if i % 10_000 == 0 {
            let (version, entries) = ol
                .rollup_entries(n + i, usize::MAX)
                .expect("rollup")
                .expect("output");
            assert!(entries.iter().all(|e| e.version() == i + 2));
            assert_eq!(version, i + 2);
            write_entries(&store, entries);
            ol = read_back(&store, &k);
        }
    }
}

#[test]
fn test_million() {
    let store = new_store();
    let k = key("bal", 1331);
    let mut ol = read_back(&store, &k);
    let n = 1_000_000u64;
    let mut commits = 0usize;
    for i in (2..=n).step_by(2) {
        add(&mut ol, i, &set_uid(i));
        ol.commit_mutation(i + 1);
        if i % 10_000 == 0 {
            ol = roll_up(&store, &ol, u64::MAX, usize::MAX).0;
        }
        commits += 1;
    }
    let l = ol.uids(&ListOptions::at(n + 1)).expect("uids");
    assert_eq!(l.uids.len(), commits);
    for (i, uid) in l.uids.iter().enumerate() {
        assert_eq!(*uid, (i as u64 + 1) * 2);
    }
}

#[test]
#[ignore = "Dgraph skips it (t.Skip): it holds several uncommitted transactions on one list, which a list of the port, owned by one request, never does"]
fn test_add_mutation_mrjn2() {}

#[test]
fn test_add_mutation_gru() {
    let store = new_store();
    let mut ol = read_back(&store, &key("question.tag", 0x01));
    add(&mut ol, 1, &set_uid(0x2b69_3088_816b_04b7));
    add(&mut ol, 1, &set_uid(0x29bf_442b_48a7_72e0));
    ol.commit_mutation(2);

    add(&mut ol, 3, &set_uid(0x38de_c821_d2ac_3a79));
    add(&mut ol, 3, &del_uid(0x2b69_3088_816b_04b7));
    ol.commit_mutation(4);
    assert_eq!(
        uids(&ol, 5, 0),
        vec![0x29bf_442b_48a7_72e0, 0x38de_c821_d2ac_3a79]
    );
}

#[test]
fn test_add_mutation_gru2() {
    let store = new_store();
    let mut ol = read_back(&store, &key("question.tag", 0x100));
    add(&mut ol, 1, &set_uid(0x02));
    add(&mut ol, 1, &set_uid(0x03));
    ol.commit_mutation(2);

    // A new tag, and the two older ones deleted.
    add(&mut ol, 3, &del_uid(0x02));
    add(&mut ol, 3, &del_uid(0x03));
    add(&mut ol, 3, &set_uid(0x04));
    ol.commit_mutation(4);

    assert_eq!(uids(&ol, 5, 0), vec![0x04]);
}

#[test]
fn test_add_and_del_mutation() {
    let store = new_store();
    let mut ol = read_back(&store, &key("dummy_key", 0x927));
    add(&mut ol, 1, &set_uid(0x02));
    ol.commit_mutation(2);

    add(&mut ol, 3, &del_uid(0x02));
    add(&mut ol, 3, &del_uid(0x02));
    ol.commit_mutation(4);
    check_uids(&ol, &[], 5);
}

fn assert_lengths(ol: &List, read_ts: u64, all: i64, after_199: i64, top: u64) {
    assert_eq!(ol.length(read_ts, 0), all);
    assert_eq!(ol.length(read_ts, 199), after_199);
    assert_eq!(ol.length(read_ts, top), 0);
}

#[test]
fn test_after_uid_count() {
    let store = new_store();
    let mut ol = read_back(&store, &single_key("value", 22));
    (100..300).for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_lengths(&ol, 1, 200, 100, 300);

    // Delete half of the edges.
    (100..300)
        .step_by(2)
        .for_each(|i| add(&mut ol, 1, &del_uid(i)));
    assert_lengths(&ol, 1, 100, 50, 300);

    // Delete them again: redundant deletes.
    (100..300)
        .step_by(2)
        .for_each(|i| add(&mut ol, 1, &del_uid(i)));
    assert_lengths(&ol, 1, 100, 50, 300);

    // Delete everything.
    (100..300).for_each(|i| add(&mut ol, 1, &del_uid(i)));
    assert_lengths(&ol, 1, 0, 0, 300);

    // Insert a quarter of the edges.
    (100..300)
        .step_by(4)
        .for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_lengths(&ol, 1, 50, 25, 300);

    // The same quarter again: no change.
    (100..300)
        .step_by(4)
        .for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_lengths(&ol, 1, 50, 25, 300);

    // Another quarter.
    (103..300)
        .step_by(4)
        .for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_lengths(&ol, 1, 100, 50, 300);
}

#[test]
fn test_after_uid_count2() {
    let store = new_store();
    let mut ol = read_back(&store, &single_key("value", 23));
    (100..300).for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_lengths(&ol, 1, 200, 100, 300);

    // Re-insert a quarter of the edges: the counts do not change.
    (100..300)
        .step_by(4)
        .for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_lengths(&ol, 1, 200, 100, 300);
}

#[test]
fn test_delete() {
    let store = new_store();
    let mut ol = read_back(&store, &single_key("value", 25));
    (1..=30).for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_eq!(ol.length(1, 0), 30);
    add(&mut ol, 1, &delete_all());
    assert_eq!(ol.length(1, 0), 0);
    ol.commit_mutation(2);
    assert_eq!(ol.length(3, 0), 0);
}

#[test]
fn test_after_uid_count_with_commit() {
    let store = new_store();
    let mut ol = read_back(&store, &single_key("value", 26));
    (100..400).for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_lengths(&ol, 1, 300, 200, 400);

    ol.commit_mutation(2);

    // The scratch layer starts afresh. Delete half of the edges.
    (100..400)
        .step_by(2)
        .for_each(|i| add(&mut ol, 3, &del_uid(i)));
    assert_lengths(&ol, 3, 150, 100, 400);

    // Redundant deletes.
    (100..400)
        .step_by(2)
        .for_each(|i| add(&mut ol, 3, &del_uid(i)));
    assert_lengths(&ol, 3, 150, 100, 400);

    // Delete everything.
    (100..400).for_each(|i| add(&mut ol, 3, &del_uid(i)));
    assert_lengths(&ol, 3, 0, 0, 400);

    // Insert a quarter of the edges.
    (100..300)
        .step_by(4)
        .for_each(|i| add(&mut ol, 3, &set_uid(i)));
    assert_lengths(&ol, 3, 50, 25, 300);

    // The same quarter again.
    (100..300)
        .step_by(4)
        .for_each(|i| add(&mut ol, 3, &set_uid(i)));
    assert_lengths(&ol, 3, 50, 25, 300);

    // Another quarter.
    (103..300)
        .step_by(4)
        .for_each(|i| add(&mut ol, 3, &set_uid(i)));
    assert_lengths(&ol, 3, 100, 50, 300);
}

fn verify_splits(splits: &[u64]) {
    assert_eq!(splits[0], 1);
    assert!(splits.windows(2).all(|w| w[1] > w[0]));
}

/// Go's `createMultiPartList`: `size` uids set one commit each, rolled up every 2,000 with a split size of 5,000
/// bytes, then rolled up at the maximum timestamp.
fn create_multi_part_list(store: &SimStore, name: &str, size: u64, facet: bool) -> (List, usize) {
    let max = 5000;
    let k = key(name, 1331);
    let mut ol = read_back(store, &k);
    let mut commits = 0;
    for i in 1..=size {
        let edge = if facet {
            with_facet(set_uid(i), &i.to_string())
        } else {
            set_uid(i)
        };
        add(&mut ol, i, &edge);
        ol.commit_mutation(i + 1);
        if i % 2000 == 0 {
            ol = roll_up(store, &ol, u64::MAX, max).0;
        }
        commits += 1;
    }
    let (version, entries) = ol
        .rollup_entries(u64::MAX, max)
        .expect("rollup")
        .expect("output");
    assert!(entries.iter().all(|e| e.version() == size + 1));
    assert_eq!(version, size + 1);
    write_entries(store, entries);
    let ol = read_back(store, &k);
    assert!(ol.plist().pack.is_none());
    assert!(ol.plist().postings.is_empty());
    assert!(!ol.plist().splits.is_empty());
    verify_splits(&ol.plist().splits);
    (ol, commits)
}

/// Go's `createAndDeleteMultiPartList`: `size` uids set, then each deleted, one commit each, rolled up every
/// 2,000 with a split size of 10,000 bytes.
fn create_and_delete_multi_part_list(store: &SimStore, size: u64) -> (List, usize) {
    let max = 10_000;
    let k = key("create-and-delete", 1331);
    let mut ol = read_back(store, &k);
    let mut commits = 0;
    for i in 1..=size {
        add(&mut ol, i, &set_uid(i));
        ol.commit_mutation(i + 1);
        if i % 2000 == 0 {
            ol = roll_up(store, &ol, u64::MAX, max).0;
        }
        commits += 1;
    }
    assert!(!ol.plist().splits.is_empty());
    verify_splits(&ol.plist().splits);

    let base = size + 1;
    for i in 1..=size {
        add(&mut ol, base + i, &del_uid(i));
        ol.commit_mutation(base + i + 1);
        if i % 2000 == 0 {
            ol = roll_up(store, &ol, u64::MAX, max).0;
        }
        commits += 1;
    }
    assert!(ol.plist().splits.is_empty());
    (ol, commits)
}

#[test]
fn test_large_plist_split() {
    let store = new_store();
    let k = key("large-plist-split", 1331);
    let mut ol = read_back(&store, &k);
    let big = vec![7u8; 300 << 10];
    for i in 1..=2u64 {
        let mut edge = with_facet(set_uid(i), &i.to_string());
        edge.value = big.clone();
        add(&mut ol, i, &edge);
        ol.commit_mutation(i + 1);
    }
    // Rolled up and not written, as in Go.
    ol.rollup_entries(u64::MAX, MAX_LIST_SIZE).expect("rollup");

    let mut ol = read_back(&store, &k);
    let value = vec![9u8; 100 << 10];
    // Uids far apart, so that the pack's blocks break at each.
    for i in 0..63u64 {
        let mut edge = with_facet(set_uid(1 << i), &i.to_string());
        edge.value = value.clone();
        add(&mut ol, i, &edge);
        ol.commit_mutation(i + 1);
    }
    let ol = roll_up(&store, &ol, u64::MAX, MAX_LIST_SIZE).0;
    assert!(ol.plist().postings.is_empty());
    assert!(!ol.plist().splits.is_empty());
    verify_splits(&ol.plist().splits);
}

#[test]
fn test_delete_star_multi_part_list() {
    let store = new_store();
    let num_edges = 10_000;
    let (mut list, _) = create_multi_part_list(&store, "delete-star", num_edges, false);
    assert_eq!(count(&list, u64::MAX), num_edges as usize);

    let read_ts = list.max_ts() + 1;
    add(&mut list, read_ts, &delete_all());
    list.commit_mutation(read_ts + 1);
    assert_eq!(count(&list, u64::MAX), 0);
}

#[test]
fn test_multi_part_list_basic() {
    let store = new_store();
    let size = 100_000;
    let (ol, commits) = create_multi_part_list(&store, "basic", size, false);
    let l = ol.uids(&ListOptions::at(size + 1)).expect("uids");
    assert_eq!(l.uids.len(), commits);
    for (i, uid) in l.uids.iter().enumerate() {
        assert_eq!(*uid, i as u64 + 1);
    }
}

#[test]
fn test_bin_split() {
    let store = new_store();
    let size = 100_000u64;
    let k = key("bin-split", 1331);
    let mut ol = read_back(&store, &k);
    for i in 1..=size {
        add(&mut ol, i, &with_facet(set_uid(i), &i.to_string()));
        ol.commit_mutation(i + 1);
    }
    let (ol, version) = roll_up(&store, &ol, u64::MAX, usize::MAX);
    assert_eq!(version, size + 1);
    assert!(ol.plist().splits.is_empty());
    assert_eq!(ol.plist().postings.len(), size as usize);

    let verify = |plist: &PostingList, starts: [u64; 2], halves: &[PostingList; 2]| {
        let all = decode(plist.pack.as_ref().unwrap(), 0);
        let mut low = decode(halves[0].pack.as_ref().unwrap(), starts[0]);
        let high = decode(halves[1].pack.as_ref().unwrap(), starts[1]);
        low.extend(high);
        assert_eq!(all, low);
        let mut postings = halves[0].postings.clone();
        postings.extend(halves[1].postings.clone());
        assert_eq!(plist.postings, postings);
        let mid = halves[1].pack.as_ref().unwrap().blocks[0].base;
        assert_eq!(starts[1], mid);
        assert!(halves[0].postings.iter().all(|p| p.uid < mid));
        assert!(halves[1].postings.iter().all(|p| p.uid >= mid));
    };
    let plist = ol.plist().clone();
    let (starts, halves) = bin_split(1, &plist);
    verify(&plist, starts, &halves);

    // The postings cut down to a third, and to none.
    let mut third = plist.clone();
    third.postings.truncate(size as usize / 3);
    let (starts, halves) = bin_split(1, &third);
    verify(&third, starts, &halves);

    let mut none = plist;
    none.postings.clear();
    let (starts, halves) = bin_split(1, &none);
    verify(&none, starts, &halves);
}

#[test]
fn test_multi_part_list_iter_after_uid() {
    let store = new_store();
    let size = 100_000u64;
    let (ol, _) = create_multi_part_list(&store, "iter-after-uid", size, false);
    let visited = uids(&ol, size + 1, 50_000);
    assert_eq!(visited.len(), 50_000);
    for (i, uid) in visited.iter().enumerate() {
        assert_eq!(*uid, 50_000 + i as u64 + 1);
    }
}

fn facet_keys(list: &List, read_ts: u64) -> Vec<String> {
    let mut facets = Vec::new();
    list.iterate(read_ts, 0, |p| {
        if let Some(f) = p.facets.first() {
            facets.push(f.key.clone());
        }
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .expect("iterate");
    facets
}

#[test]
fn test_multi_part_list_with_postings() {
    let store = new_store();
    let size = 100_000u64;
    let (ol, commits) = create_multi_part_list(&store, "with-postings", size, true);
    let facets = facet_keys(&ol, size + 1);
    assert_eq!(facets.len(), commits);
    for (i, facet) in facets.iter().enumerate() {
        assert_eq!(*facet, (i + 1).to_string());
    }
}

#[test]
fn test_multi_part_list_marshal() {
    let store = new_store();
    let size = 100_000u64;
    let (ol, _) = create_multi_part_list(&store, "marshal", size, false);

    let (_, mut entries) = ol
        .rollup_entries(u64::MAX, 5000)
        .expect("rollup")
        .expect("output");
    assert_eq!(entries.len(), ol.plist().splits.len() + 1);
    entries.sort_by(|a, b| a.key().cmp(b.key()));
    for (i, start) in ol.plist().splits.iter().enumerate() {
        let part_key = split_key_of(entries[0].key(), *start).expect("split key");
        assert_eq!(entries[i + 1].key(), part_key);
        assert_eq!(
            entries[i + 1].value(),
            Some(marshal(&ol.parts()[i]).as_slice())
        );
        assert_eq!(entries[i + 1].meta(), BIT_COMPLETE_POSTING);
        assert_eq!(entries[i + 1].version(), ol.min_ts());
    }
}

#[test]
fn test_multi_part_list_write_to_disk() {
    let store = new_store();
    let size = 100_000u64;
    let (original, commits) = create_multi_part_list(&store, "write-to-disk", size, false);

    let (new_list, _) = roll_up(&store, &original, u64::MAX, 5000);
    let opt = ListOptions::at(size + 1);
    let original_uids = original.uids(&opt).expect("uids");
    let new_uids = new_list.uids(&opt).expect("uids");
    assert_eq!(original_uids.uids.len(), commits);
    assert_eq!(original_uids, new_uids);
}

#[test]
fn test_multi_part_list_delete() {
    let store = new_store();
    let size = 10_000u64;
    let (ol, commits) = create_and_delete_multi_part_list(&store, size);
    assert_eq!(commits, size as usize * 2);
    assert_eq!(count(&ol, u64::MAX), 0);

    let (_, entries) = ol
        .rollup_entries(u64::MAX, 10_000)
        .expect("rollup")
        .expect("output");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].meta(), BIT_EMPTY_POSTING);
    assert_eq!(entries[0].version(), ol.min_ts());
}

#[test]
fn test_multi_part_list_delete_and_add() {
    let store = new_store();
    let size = 100_000u64;
    let max = 5000;
    let k = key("delete-and-add", 1331);
    let mut ol = read_back(&store, &k);
    for i in 1..=size {
        add(&mut ol, i, &set_uid(i));
        ol.commit_mutation(i + 1);
        if i % 2000 == 0 {
            ol = roll_up(&store, &ol, u64::MAX, max).0;
        }
    }
    let all = ol.uids(&ListOptions::at(u64::MAX)).expect("uids");
    assert_eq!(all.uids, (1..=size).collect::<Vec<_>>());

    // Delete the first half.
    let base = size + 1;
    for i in 1..=size / 2 {
        add(&mut ol, base + i, &del_uid(i));
        ol.commit_mutation(base + i + 1);
        if i % 2000 == 0 {
            ol = roll_up(&store, &ol, u64::MAX, max).0;
        }
    }
    let (_, entries) = ol
        .rollup_entries(u64::MAX, max)
        .expect("rollup")
        .expect("output");
    assert!(entries.iter().all(|e| e.version() == base + 1 + size / 2));
    write_entries(&store, entries);
    let mut ol = read_back(&store, &k);
    let half = ol.uids(&ListOptions::at(u64::MAX)).expect("uids");
    assert_eq!(half.uids, (size / 2 + 1..=size).collect::<Vec<_>>());

    // Add the deleted half back.
    let base = 2 * size + 1;
    for i in 1..=size / 2 {
        add(&mut ol, base + i, &set_uid(i));
        ol.commit_mutation(base + i + 1);
        if i % 2000 == 0 {
            ol = roll_up(&store, &ol, u64::MAX, max).0;
        }
    }
    let ol = roll_up(&store, &ol, u64::MAX, max).0;
    let all = ol.uids(&ListOptions::at(u64::MAX)).expect("uids");
    assert_eq!(all.uids, (1..=size).collect::<Vec<_>>());
}

#[test]
fn test_single_list_rollup() {
    let store = new_store();
    let size = 100_000u64;
    let (ol, commits) = create_multi_part_list(&store, "single-list-rollup", size, true);
    let facets = facet_keys(&ol, size + 1);
    assert_eq!(facets.len(), commits);
    for (i, facet) in facets.iter().enumerate() {
        assert_eq!(*facet, (i + 1).to_string());
    }

    let (bl, entry) = ol
        .to_backup_posting_list()
        .expect("backup")
        .expect("output");
    assert_eq!(entry.meta(), BIT_COMPLETE_POSTING);
    let plist = from_backup_posting_list(&bl).expect("from backup");
    assert!(plist.splits.is_empty());
    assert_eq!(decode(plist.pack.as_ref().unwrap(), 0).len(), size as usize);
}

#[test]
fn test_list_delete_marker() {
    let store = new_store();
    let mut ol = read_back(&store, &key("delete-marker", 13345));
    add(&mut ol, 1, &set_uid(1));
    ol.commit_mutation(2);
    add(&mut ol, 3, &delete_all());
    assert_eq!(ol.mutation().populate_delete_all(3), 3);
}

/// Go's split tests: `size` uids with facets, one commit each and no rollup until the end, split at Dgraph's size.
fn recursively_split(store: &SimStore, name: &str, size: u64) -> List {
    let k = key(name, 1333);
    let mut ol = read_back(store, &k);
    for i in 1..=size {
        add(&mut ol, i, &with_facet(set_uid(i), &i.to_string()));
        ol.commit_mutation(i + 1);
    }
    let ol = roll_up(store, &ol, u64::MAX, MAX_LIST_SIZE).0;
    assert!(ol.plist().splits.len() > 2);
    ol
}

#[test]
fn test_split_length() {
    let store = new_store();
    let size = 100_000u64;
    let ol = recursively_split(&store, "split-length", size);
    assert_eq!(ol.get_length(size + 10).expect("length"), size as i64);
}

#[test]
fn test_recursive_splits() {
    let store = new_store();
    let size = 100_000u64;
    let ol = recursively_split(&store, "recursive-splits", size);
    let facets = facet_keys(&ol, size + 1);
    assert_eq!(facets.len(), size as usize);
    for (i, facet) in facets.iter().enumerate() {
        assert_eq!(*facet, (i + 1).to_string());
    }
}

/// Uids of `list` at `read_ts`.
fn uids_at(list: &List, read_ts: u64) -> Vec<u64> {
    list.uids(&ListOptions::at(read_ts)).expect("uids").uids
}

#[test]
fn test_calculated_uids_respect_read_ts() {
    // Issue #9795: a read below the newest commit sees the list as of its timestamp. The port has no calculated-uid
    // cache, so the reads take the path Dgraph takes without it.
    let mut l = List::empty(key("calculatedUidsReadTs", 7));
    [3, 4, 10].iter().for_each(|&u| add(&mut l, 5, &set_uid(u)));
    l.commit_mutation(10);
    add(&mut l, 15, &del_uid(4));
    add(&mut l, 15, &set_uid(13));
    l.commit_mutation(20);

    assert_eq!(uids_at(&l, 20), vec![3, 10, 13]);
    assert_eq!(uids_at(&l, 25), vec![3, 10, 13]);
    assert_eq!(uids_at(&l, 15), vec![3, 4, 10]);
    assert!(uids_at(&l, 5).is_empty());
}

#[test]
fn test_calculated_uids_skipped_for_bounded_reads() {
    // Without the calculated-uid cache, Go's poisoned memo has no counterpart: every read returns the list's uids.
    let mut l = List::empty(key("calculatedUidsBoundedReads", 7));
    [2, 3, 4].iter().for_each(|&u| add(&mut l, 5, &set_uid(u)));
    l.commit_mutation(10);

    assert_eq!(uids_at(&l, 10), vec![2, 3, 4]);
    let worker = l
        .uids(&ListOptions::at(10).first(i64::from(i32::MAX)))
        .expect("uids");
    assert_eq!(worker.uids, vec![2, 3, 4]);
    let first = l.uids(&ListOptions::at(10).first(1)).expect("uids");
    assert_eq!(first.uids, vec![2]);
    let negative = l.uids(&ListOptions::at(10).first(-1)).expect("uids");
    assert_eq!(negative.uids, vec![2, 3, 4]);
    let intersect = l
        .uids(&ListOptions::at(10).intersect(ultragraphdb_proto_pb::pb::List { uids: vec![3] }))
        .expect("uids");
    assert_eq!(intersect.uids, vec![3]);
}

#[test]
fn test_uids_does_not_apply_a_negative_first() {
    let build = |n: u64| {
        let mut l = List::empty(key(&format!("negativeFirstNotApplied{n}"), 7));
        [2, 4, 6, 8, 10]
            .iter()
            .for_each(|&u| add(&mut l, 5, &set_uid(u)));
        l.commit_mutation(10);
        l
    };
    let all = vec![2u64, 4, 6, 8, 10];
    for (n, first) in [-1i64, -2, -4, -5, -9, i64::from(i32::MIN), i64::MIN]
        .into_iter()
        .enumerate()
    {
        let got = build(n as u64)
            .uids(&ListOptions::at(10).first(first))
            .expect("uids");
        assert_eq!(got.uids, all, "first: {first}");
    }
    // The after-uid still applies.
    let got = build(99)
        .uids(&ListOptions::at(10).first(-2).after(4))
        .expect("uids");
    assert_eq!(got.uids, vec![6, 8, 10]);
}

#[test]
fn test_value_postings_are_kept_beside_the_uids() {
    // encode keeps a posting with facets or a value; a bare reference is only a uid in the pack.
    let mut l = List::empty(key("kept", 1));
    add(&mut l, 1, &set_uid(5));
    add(&mut l, 1, &set_value("v"));
    l.commit_mutation(2);
    let out = l
        .rollup(3, true, MAX_LIST_SIZE)
        .expect("rollup")
        .expect("output");
    assert_eq!(out.plist().postings.len(), 1);
    assert_eq!(
        out.plist().postings[0].posting_type,
        posting::PostingType::Value as i32
    );
    assert_eq!(
        decode(out.plist().pack.as_ref().unwrap(), 0),
        vec![5, fingerprint64(b"v")]
    );
}
