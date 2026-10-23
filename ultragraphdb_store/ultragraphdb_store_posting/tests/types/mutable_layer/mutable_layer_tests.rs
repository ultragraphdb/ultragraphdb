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
use ultragraphdb_store_posting::{DEL, MutableLayer, OVR, SET};

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
