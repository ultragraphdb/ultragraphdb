/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The merge of the deltas over the immutable layer, at the boundaries of the read timestamp, the uids and the
//! layers: an empty list, one posting, the complete version's own timestamp, the largest uid, a delete over a set
//! and a commit at the request's own read timestamp.

use std::ops::ControlFlow;

use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_store_posting::{List, ListOptions, PostingError, PostingErrorEnum};

use crate::utils::helpers::{
    add, del_uid, delete_all, key, new_store, roll_up, set_uid, uids, with_facet,
};

/// The forward key of predicate 7 for subject 1 in the default graph of the root namespace.
fn known_key() -> Vec<u8> {
    QuadKey::Forward {
        namespace: 0,
        graph: 0,
        predicate: 7,
        subject: 1,
    }
    .to_bytes()
}

fn facets(list: &List, read_ts: u64) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    list.iterate(read_ts, 0, |p| {
        out.push((
            p.uid,
            p.facets.first().map(|f| f.key.clone()).unwrap_or_default(),
        ));
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .unwrap();
    out
}

#[test]
fn test_an_empty_list_reads_empty_everywhere() {
    let l = List::empty(key("iter-empty", 1));
    for read_ts in [0, 1, u64::MAX] {
        assert!(uids(&l, read_ts, 0).is_empty());
        assert_eq!(l.length(read_ts, 0), 0);
        assert!(l.is_empty(read_ts, 0).unwrap());
        assert_eq!(l.get_length(read_ts).unwrap(), 0);
        assert_eq!(l.find_posting(read_ts, 1).unwrap(), None);
        assert!(l.static_value(read_ts).is_none());
        assert!(l.uids(&ListOptions::at(read_ts)).unwrap().uids.is_empty());
        assert_eq!(l.pick_postings(read_ts), (0, Vec::new()));
    }
    assert_eq!(l.approx_len(), 0);
}

#[test]
fn test_one_posting_is_read_from_its_commit_on() {
    let mut l = List::empty(key("iter-one", 1));
    add(&mut l, 4, &set_uid(9));
    l.commit_mutation(5);
    assert!(uids(&l, 4, 0).is_empty(), "not visible below its commit");
    assert_eq!(uids(&l, 5, 0), vec![9]);
    assert_eq!(uids(&l, 5, 8), vec![9]);
    assert!(
        uids(&l, 5, 9).is_empty(),
        "the after-uid itself is excluded"
    );
    assert_eq!((l.length(5, 0), l.get_length(5).unwrap()), (1, 1));
    assert_eq!((l.min_ts(), l.max_ts()), (0, 5));
}

#[test]
fn test_a_read_at_the_complete_version_goes_ahead_and_one_below_is_refused() {
    let l = List::new(known_key(), PostingList::default(), 5);
    assert!(uids(&l, 5, 0).is_empty());
    let error = l
        .iterate(4, 0, |_| Ok::<_, PostingError>(ControlFlow::Continue(())))
        .unwrap_err();
    assert_eq!(
        error.0,
        PostingErrorEnum::ReadBelowMinTs {
            read_ts: 4,
            min_ts: 5,
            key: known_key()
        }
    );
    // Go 1.27 probe: errors.Errorf("readTs: %d less than minTs: %d for key: %q", 4, 5, key) over the same key.
    assert_eq!(
        error.to_string(),
        "readTs: 4 less than minTs: 5 for key: \"\\x10\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\a\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x01\""
    );
    assert_eq!(
        l.length(4, 0),
        -1,
        "Dgraph's Length reports a failed iteration as -1"
    );
    assert_eq!(l.get_posting_and_length(4, 0, 1), (-1, None));
}

#[test]
fn test_a_delete_all_below_the_complete_version_is_refused_and_one_at_it_hides_the_list() {
    let store = new_store();
    let mut l = List::empty(key("iter-delete-below", 1));
    add(&mut l, 1, &set_uid(3));
    l.commit_mutation(2);
    let (rolled, _) = roll_up(&store, &l, 4, usize::MAX);
    assert_eq!(
        rolled.min_ts(),
        3,
        "a rollup at 4 writes one above the commit at 2"
    );

    let below = rolled.committed(
        2,
        PostingList {
            postings: vec![delete_all()],
            ..PostingList::default()
        },
    );
    let error = below.is_empty(5, 0).unwrap_err();
    assert_eq!(
        error.0,
        PostingErrorEnum::DeleteBelowMinTs {
            delete_below_ts: 2,
            min_ts: 3
        }
    );
    assert_eq!(
        error.to_string(),
        "deleteBelowTs (2) must be greater than the minTs in the list (3)"
    );

    // The transaction starting at the rollup's version may delete all at it.
    let at = rolled.committed(
        3,
        PostingList {
            postings: vec![delete_all()],
            ..PostingList::default()
        },
    );
    assert!(uids(&at, 3, 0).is_empty());
    assert_eq!(at.get_length(3).unwrap(), 0);
    assert_eq!(uids(&rolled, 3, 0), vec![3]);
}

#[test]
fn test_only_the_newest_version_of_a_posting_counts() {
    let mut l = List::empty(key("iter-newest", 1));
    add(&mut l, 1, &with_facet(set_uid(5), "old"));
    l.commit_mutation(2);
    add(&mut l, 3, &with_facet(set_uid(5), "new"));
    l.commit_mutation(4);
    assert_eq!(facets(&l, 3), vec![(5, "old".to_string())]);
    assert_eq!(facets(&l, 4), vec![(5, "new".to_string())]);
    assert_eq!(l.length(4, 0), 1);
}

#[test]
fn test_a_delete_hides_a_set_of_the_immutable_layer_and_a_later_set_restores_it() {
    let store = new_store();
    let mut l = List::empty(key("iter-delete-over-set", 1));
    [1, 2, 3].iter().for_each(|&u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let (mut l, _) = roll_up(&store, &l, u64::MAX, usize::MAX);
    add(&mut l, 3, &del_uid(2));
    add(&mut l, 3, &del_uid(7));
    l.commit_mutation(4);
    assert_eq!(uids(&l, 4, 0), vec![1, 3]);
    assert_eq!(l.length(4, 0), 2);
    assert_eq!(l.find_posting(4, 2).unwrap(), None);
    assert_eq!(
        uids(&l, 3, 0),
        vec![1, 2, 3],
        "the delete is not visible below its commit"
    );
    add(&mut l, 5, &set_uid(2));
    l.commit_mutation(6);
    assert_eq!(uids(&l, 6, 0), vec![1, 2, 3]);
}

/// Deviation: postings of one uid are ordered by their layer, the request's own newest, where Dgraph orders them by
/// an effective timestamp that ties between a commit at the read timestamp and the request's own write.
#[test]
fn test_a_requests_own_write_is_newer_than_a_commit_at_its_read_timestamp() {
    let mut l = List::empty(key("iter-tie", 1));
    add(&mut l, 4, &set_uid(3));
    l.commit_mutation(5);
    add(&mut l, 5, &del_uid(3));
    assert!(
        uids(&l, 5, 0).is_empty(),
        "the request deleted the uid committed at its read timestamp"
    );
    let mut other = List::empty(key("iter-tie", 2));
    add(&mut other, 4, &del_uid(3));
    other.commit_mutation(5);
    add(&mut other, 5, &set_uid(3));
    assert_eq!(uids(&other, 5, 0), vec![3]);
}

#[test]
fn test_the_largest_uid_is_read_from_the_deltas_and_from_the_pack() {
    let store = new_store();
    let mut l = List::empty(key("iter-max", 1));
    add(&mut l, 1, &set_uid(1));
    add(&mut l, 1, &set_uid(u64::MAX));
    l.commit_mutation(2);
    assert_eq!(uids(&l, 2, 0), vec![1, u64::MAX]);
    assert_eq!(uids(&l, 2, u64::MAX - 1), vec![u64::MAX]);
    let (rolled, _) = roll_up(&store, &l, u64::MAX, usize::MAX);
    assert_eq!(uids(&rolled, 2, 0), vec![1, u64::MAX]);
    assert_eq!(uids(&rolled, 2, u64::MAX - 1), vec![u64::MAX]);
    assert!(uids(&rolled, 2, u64::MAX).is_empty());
    assert_eq!(
        rolled.find_posting(2, u64::MAX).unwrap().map(|p| p.uid),
        Some(u64::MAX)
    );
}

#[test]
fn test_iteration_stops_where_the_callback_breaks_and_passes_its_error() {
    let mut l = List::empty(key("iter-stop", 1));
    (1..=5).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let mut seen = Vec::new();
    l.iterate(2, 0, |p| {
        seen.push(p.uid);
        Ok::<_, PostingError>(if p.uid == 2 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        })
    })
    .unwrap();
    assert_eq!(seen, vec![1, 2]);
    let error = l
        .iterate(2, 0, |_| -> Result<ControlFlow<()>, PostingError> {
            Err(PostingError::TsTooOld())
        })
        .unwrap_err();
    assert_eq!(error.0, PostingErrorEnum::TsTooOld);
}

/// Iterating reports at trace level when it read fewer postings than it skipped as deleted: rollups lag behind the
/// deletes. One set and two deletes: 3 postings, 66 percent deleted (2 × 100 / 3, rounded down).
#[test]
fn test_a_read_of_mostly_deleted_postings_is_reported() {
    let capture = ultragraphdb_store_sim::LogCapture::new();
    capture.capture(|| {
        let mut l = List::empty(key("iter-deleted", 1));
        add(&mut l, 1, &set_uid(1));
        add(&mut l, 1, &del_uid(2));
        add(&mut l, 1, &del_uid(3));
        l.commit_mutation(2);
        assert_eq!(uids(&l, 2, 0), vec![1]);
        let mut even = List::empty(key("iter-deleted", 2));
        add(&mut even, 1, &set_uid(1));
        add(&mut even, 1, &del_uid(2));
        even.commit_mutation(2);
        assert_eq!(uids(&even, 2, 0), vec![1]);
    });
    let events = capture.events();
    assert_eq!(
        events.len(),
        1,
        "as many read as deleted is not reported: {events:?}"
    );
    assert_eq!(events[0].level(), tracing::Level::TRACE);
    assert_eq!(events[0].field("total"), Some("3"));
    assert_eq!(events[0].field("percent_deleted"), Some("66"));
}

/// A property over a split list: a read after any uid, part starts and part ends included, yields exactly the uids
/// above it, and each uid is found where it is held.
#[test]
fn test_a_read_after_every_uid_of_a_split_list_yields_the_uids_above_it() {
    let store = new_store();
    let mut l = List::empty(key("iter-sweep", 1));
    // Odd uids, so that a read can start between two held uids as well as at one.
    (1..=1200u64).for_each(|i| add(&mut l, 1, &set_uid(2 * i - 1)));
    l.commit_mutation(2);
    let (l, _) = roll_up(&store, &l, u64::MAX, 300);
    let splits = l.part_splits();
    assert!(splits.len() > 3, "{splits:?}");
    for after in 0..=2400u64 {
        let expected: Vec<u64> = (after / 2 + 1..=1200)
            .map(|i| 2 * i - 1)
            .filter(|&u| u > after)
            .collect();
        let got = uids(&l, 2, after);
        assert!(
            got == expected,
            "after {after}: first {:?}, {} uids",
            got.first(),
            got.len()
        );
        let held = after % 2 == 1;
        assert_eq!(
            l.find_posting(2, after).unwrap().is_some(),
            held && after > 0,
            "uid {after}"
        );
    }
}
