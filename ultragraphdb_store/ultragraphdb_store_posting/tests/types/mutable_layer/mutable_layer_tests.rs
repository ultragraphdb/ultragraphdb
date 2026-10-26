/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list_test.go (TestMutableMap). Changed for ultragraphdb: translated to Rust;
 * Go's cached `deleteAllMarker`, `length` and `committedUidsTime` are read through what the port computes them
 * from: the marker at the maximum timestamp, the committed length, and the newest commit timestamp.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::{Posting, PostingList};
use ultragraphdb_store_posting::{DEL, List, MutableLayer, OVR, SET};

use crate::utils::helpers::{del_uid, delete_all, key, set_uid, with_facet};

fn pl(start_ts: u64, commit_ts: u64, uid: u64, op: u32) -> PostingList {
    PostingList {
        postings: vec![Posting {
            uid,
            op,
            start_ts,
            commit_ts,
            ..Posting::default()
        }],
        commit_ts,
        ..PostingList::default()
    }
}

fn delete_all_pl(start_ts: u64, commit_ts: u64) -> PostingList {
    PostingList {
        postings: vec![Posting {
            value: b"_STAR_ALL".to_vec(),
            op: DEL,
            start_ts,
            commit_ts,
            ..Posting::default()
        }],
        commit_ts,
        ..PostingList::default()
    }
}

#[test]
fn test_mutable_map_populate_delete_all() {
    let mut ml = MutableLayer::new();
    ml.insert_committed_postings(pl(10, 11, 1, SET));
    ml.insert_committed_postings(delete_all_pl(8, 9));
    ml.insert_committed_postings(pl(6, 7, 1, SET));
    ml.insert_committed_postings(delete_all_pl(4, 5));
    ml.insert_committed_postings(pl(2, 3, 1, SET));
    ml.insert_committed_postings(delete_all_pl(1, 2));

    assert_eq!(ml.populate_delete_all(u64::MAX), 9);
    assert_eq!(ml.committed_len(), 1);

    ml.set_current_entries(11, delete_all_pl(11, 0));
    assert_eq!(ml.committed_len(), 1);

    assert_eq!(ml.populate_delete_all(11), 11);
    assert_eq!(ml.populate_delete_all(10), 9);
    assert_eq!(ml.populate_delete_all(5), 5);
    assert_eq!(ml.populate_delete_all(4), 2);
}

#[test]
fn test_mutable_map_current_entries_has_delete_all() {
    let mut ml = MutableLayer::new();
    ml.insert_committed_postings(pl(1, 2, 1, SET));
    ml.set_current_entries(3, delete_all_pl(3, 0));

    assert_eq!(ml.populate_delete_all(3), 3);
    assert_eq!(ml.committed_uids_time(), 2);
    assert_eq!(ml.committed_len(), 1);

    let (search_further, pos) = ml.find_posting(3, 1);
    assert!(!search_further);
    assert!(pos.is_none());

    let (search_further, pos) = ml.find_posting(2, 1);
    assert!(!search_further);
    assert_eq!(pos.map(|p| p.commit_ts), Some(2));

    assert_eq!(ml.list_len(1), 0);
    assert_eq!(ml.list_len(2), 1);
    assert_eq!(ml.list_len(3), 0);
}

#[test]
fn test_mutable_map_data_in_committed_uids_but_found_posting() {
    let mut ml = MutableLayer::new();
    ml.insert_committed_postings(delete_all_pl(4, 5));
    ml.insert_committed_postings(pl(2, 3, 1, OVR));
    ml.insert_committed_postings(pl(1, 2, 1, SET));
    ml.set_current_entries(7, pl(7, 0, 2, SET));

    assert_eq!(ml.committed_uids_time(), 5);
    assert_eq!(ml.committed_len(), 0);

    let (search_further, pos) = ml.find_posting(2, 1);
    assert!(!search_further);
    assert_eq!(pos.map(|p| p.start_ts), Some(1));

    let (search_further, pos) = ml.find_posting(3, 1);
    assert!(!search_further);
    assert_eq!(pos.map(|p| p.start_ts), Some(2));

    let (search_further, pos) = ml.find_posting(8, 1);
    assert!(!search_further);
    assert!(pos.is_none());

    let (search_further, pos) = ml.find_posting(7, 2);
    assert!(!search_further);
    assert_eq!(pos.map(|p| p.start_ts), Some(7));

    // A read at another timestamp than the scratch layer's does not see it.
    let (search_further, pos) = ml.find_posting(8, 2);
    assert!(!search_further);
    assert!(pos.is_none());

    assert_eq!(ml.list_len(1), 0);
    assert_eq!(ml.list_len(2), 1);
    assert_eq!(ml.list_len(3), 1);
    assert_eq!(ml.list_len(5), 0);
    assert_eq!(ml.list_len(7), 1);
    assert_eq!(ml.list_len(8), 0);
}

#[test]
fn test_mutable_map_no_current_entries_set() {
    let mut ml = MutableLayer::new();
    ml.insert_committed_postings(pl(16, 17, 1, DEL));
    ml.insert_committed_postings(pl(13, 14, 1, OVR));
    ml.insert_committed_postings(pl(10, 11, 1, SET));
    ml.insert_committed_postings(delete_all_pl(8, 9));
    ml.insert_committed_postings(pl(5, 6, 1, DEL));
    ml.insert_committed_postings(pl(1, 2, 1, SET));

    for (ts, len) in [
        (1, 0),
        (2, 1),
        (6, 0),
        (9, 0),
        (11, 1),
        (14, 1),
        (17, 0),
        (20, 0),
    ] {
        assert_eq!(ml.list_len(ts), len, "at {ts}");
    }
}

#[test]
fn test_layers_and_read_ts() {
    let mut ml = MutableLayer::new();
    assert!(ml.is_empty());
    assert_eq!(ml.committed_uids_time(), u64::MAX);
    ml.insert_committed_postings(pl(1, 2, 1, SET));
    ml.set_ts(4);
    assert_eq!(ml.read_ts(), 4);
    assert!(ml.current_entries().is_none());
    assert!(ml.stacked().is_empty());
    assert_eq!(ml.committed().len(), 1);
    assert_eq!(ml.len(), 1);
    // A committed posting takes its delta's commit timestamp.
    assert_eq!(ml.committed()[&2].postings[0].commit_ts, 2);
}

fn ops(list: &List) -> Vec<(u64, u32)> {
    list.delta()
        .map(|d| d.postings.iter().map(|p| (p.uid, p.op)).collect())
        .unwrap_or_default()
}

#[test]
fn test_a_posting_replaces_the_one_of_its_identity_in_the_scratch_layer() {
    let mut l = List::empty(key("insert", 1));
    l.set_ts(5);
    l.add_posting(set_uid(7), false);
    l.add_posting(set_uid(8), false);
    l.add_posting(del_uid(7), false);
    assert_eq!(
        ops(&l),
        vec![(7, DEL), (8, SET)],
        "without a count index a delete replaces the set"
    );
    l.add_posting(del_uid(7), true);
    assert_eq!(
        ops(&l),
        vec![(7, DEL), (8, SET)],
        "a delete of a delete stays one"
    );
    assert!(l.delta().unwrap().postings.iter().all(|p| p.start_ts == 5));
}

#[test]
fn test_with_a_count_index_the_identities_are_reindexed_after_a_removal() {
    let mut l = List::empty(key("insert-reindex", 1));
    l.set_ts(5);
    l.add_posting(set_uid(7), true);
    l.add_posting(set_uid(8), true);
    l.add_posting(del_uid(7), true);
    assert_eq!(ops(&l), vec![(8, SET)]);
    // 8 moved to the front: its overwrite must find it there rather than add a second posting.
    l.add_posting(with_facet(set_uid(8), "again"), true);
    assert_eq!(ops(&l), vec![(8, SET)]);
    assert_eq!(l.delta().unwrap().postings[0].facets[0].key, "again");
    l.add_posting(del_uid(8), true);
    assert!(ops(&l).is_empty());
}

#[test]
fn test_an_overwrite_replaces_a_delete_and_stays_an_overwrite() {
    let mut l = List::empty(key("insert-ovr", 1));
    l.set_ts(5);
    l.add_posting(del_uid(3), false);
    l.add_posting(
        Posting {
            op: OVR,
            ..set_uid(3)
        },
        false,
    );
    assert_eq!(ops(&l), vec![(3, OVR)]);
    l.add_posting(
        Posting {
            op: OVR,
            ..set_uid(4)
        },
        false,
    );
    l.add_posting(
        Posting {
            op: OVR,
            ..set_uid(4)
        },
        false,
    );
    assert_eq!(ops(&l), vec![(3, OVR), (4, OVR)]);
}

#[test]
fn test_postings_of_uid_zero_are_kept_apart() {
    // A posting of uid 0 is never indexed, so each one is added.
    let mut l = List::empty(key("insert-zero", 1));
    l.set_ts(5);
    l.add_posting(set_uid(0), false);
    l.add_posting(set_uid(0), false);
    assert_eq!(ops(&l), vec![(0, SET), (0, SET)]);
}

#[test]
fn test_a_delete_all_replaces_the_scratch_layer() {
    let mut l = List::empty(key("insert-delete-all", 1));
    l.set_ts(5);
    l.add_posting(set_uid(1), false);
    l.add_posting(set_uid(2), false);
    l.add_posting(delete_all(), false);
    assert_eq!(ops(&l), vec![(u64::MAX, DEL)]);
    l.add_posting(set_uid(3), false);
    assert_eq!(ops(&l), vec![(u64::MAX, DEL), (3, SET)]);
    assert_eq!(l.mutation().populate_delete_all(5), 5);
    assert_eq!(l.mutation().list_len(5), 1);
}

#[test]
fn test_the_stacked_layers_and_the_scratch_layer_are_seen_at_the_read_timestamp_alone() {
    let mut l = List::empty(key("layers", 1));
    l.set_ts(3);
    l.add_posting(set_uid(1), false);
    l.commit_mutation(4);
    l.set_ts(6);
    l.add_posting(set_uid(2), false);
    let ml = l.mutation();
    assert_eq!(ml.len(), 2, "one committed delta and the scratch layer");
    assert_eq!(ml.committed_uids_time(), 4);
    assert_eq!((ml.list_len(4), ml.list_len(5), ml.list_len(6)), (1, 1, 2));
    assert_eq!(ml.committed_len(), 1, "the scratch layer is not committed");
    assert_eq!(
        ml.find_posting(5, 2),
        (true, None),
        "not visible: search the immutable layer"
    );
}
