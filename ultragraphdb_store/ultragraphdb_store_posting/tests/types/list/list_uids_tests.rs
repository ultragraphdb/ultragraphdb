/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `List::uids` takes one of three paths: the pack intersected directly (no deltas, no parts), each uid of a short
//! intersection looked up, or the list iterated. Each is pinned here, with the read timestamp and the counts at their
//! boundaries.

use std::ops::ControlFlow;

use ultragraphdb_core_codec::{List as UidList, Posting, PostingType};
use ultragraphdb_store_posting::{List, ListOptions, PostingError, PostingErrorEnum};

use crate::utils::helpers::{
    add, del_uid, key, new_store, roll_up, set_uid, set_value, with_facet,
};

fn list_of(uids: &[u64]) -> UidList {
    UidList::new(uids.to_vec())
}

/// A rolled-up list of the uids 2, 4, …, 600, complete at its rollup's version, with no deltas: 300 uids in two
/// blocks of 256, so its approximate length is 512 (2 × 256).
fn rolled_evens() -> List {
    let store = new_store();
    let mut l = List::empty(key("uids-evens", 1));
    (1..=300).for_each(|i| add(&mut l, 1, &set_uid(2 * i)));
    l.commit_mutation(2);
    roll_up(&store, &l, u64::MAX, usize::MAX).0
}

#[test]
fn test_the_pack_is_intersected_directly_without_deltas() {
    let l = rolled_evens();
    assert!(l.mutation().is_empty() && l.part_splits().is_empty());
    assert_eq!(
        l.min_ts(),
        2,
        "the rollup at the maximum timestamp takes the commit's version"
    );
    let got = l
        .uids(&ListOptions::at(2).intersect(list_of(&[1, 4, 8, 9, 600, 601])))
        .unwrap();
    assert_eq!(got.uids(), vec![4, 8, 600]);
    // Go 1.27 probe of algo.IntersectCompressedWith over the same pack: [4 8]. The direct path seeks with SeekStart,
    // which keeps the after-uid itself, where iterating starts above it (below): a Dgraph inconsistency kept.
    let after = l
        .uids(
            &ListOptions::at(2)
                .intersect(list_of(&[1, 4, 8, 9]))
                .after(4),
        )
        .unwrap();
    assert_eq!(after.uids(), vec![4, 8]);
    let mut with_delta = l.clone();
    add(&mut with_delta, 2, &set_uid(1));
    with_delta.commit_mutation(3);
    // With a delta, four uids are fewer than the approximate length (1 + 512), so each is looked up, and the lookup
    // ignores the after-uid altogether, as Go's (list.go:1903-1918, findPostingWithItr per uid).
    let looked_up = with_delta
        .uids(
            &ListOptions::at(3)
                .intersect(list_of(&[1, 4, 8, 9]))
                .after(4),
        )
        .unwrap();
    assert_eq!(looked_up.uids(), vec![1, 4, 8]);
}

#[test]
fn test_an_intersection_below_the_complete_version_is_too_old() {
    let l = rolled_evens();
    let error = l
        .uids(&ListOptions::at(1).intersect(list_of(&[4])))
        .unwrap_err();
    assert_eq!(error.0, PostingErrorEnum::TsTooOld);
    assert_eq!(error.to_string(), "Transaction is too old");
}

#[test]
fn test_a_short_intersection_looks_each_uid_up() {
    let mut l = rolled_evens();
    // A delta makes the direct path unavailable; two uids are fewer than the approximate length, 1 + 512.
    add(&mut l, 3, &del_uid(4));
    add(&mut l, 3, &set_uid(5));
    l.commit_mutation(4);
    assert_eq!(
        l.approx_len(),
        513,
        "one delta layer plus two blocks of 256"
    );
    let got = l
        .uids(&ListOptions::at(4).intersect(list_of(&[4, 5, 6, 7])))
        .unwrap();
    assert_eq!(
        got.uids(),
        vec![5, 6],
        "4 deleted, 5 set, 6 held, 7 never held"
    );
    let before = l
        .uids(&ListOptions::at(3).intersect(list_of(&[4, 5, 6])))
        .unwrap();
    assert_eq!(
        before.uids(),
        vec![4, 6],
        "the delta at 4 is not visible at 3"
    );
}

#[test]
fn test_a_long_intersection_iterates_only_its_range() {
    // A list of deltas alone: its approximate length is its one layer, so an intersection of two or more uids
    // iterates, reading only from the intersection's first to its last uid.
    let mut l = List::empty(key("uids-range", 1));
    (1..=10).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    assert_eq!(l.approx_len(), 1);
    let got = l
        .uids(&ListOptions::at(2).intersect(list_of(&[3, 7, 12])))
        .unwrap();
    assert_eq!(got.uids(), vec![3, 7]);
    let after = l
        .uids(&ListOptions::at(2).intersect(list_of(&[3, 7, 12])).after(3))
        .unwrap();
    assert_eq!(
        after.uids(),
        vec![7],
        "iterating starts above the after-uid"
    );
    let first = l
        .uids(&ListOptions::at(2).intersect(list_of(&[3, 5, 7])).first(2))
        .unwrap();
    assert_eq!(
        first.uids(),
        vec![3, 5],
        "the count applies after the intersection"
    );
    let empty = l.uids(&ListOptions::at(2).intersect(list_of(&[]))).unwrap();
    assert!(
        empty.uids().is_empty(),
        "an empty intersection holds nothing"
    );
}

#[test]
fn test_a_positive_count_at_and_around_the_list_length() {
    let mut l = List::empty(key("uids-first", 1));
    (1..=5).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    for (first, expected) in [
        (4, vec![1, 2, 3, 4]),
        (5, vec![1, 2, 3, 4, 5]),
        (6, vec![1, 2, 3, 4, 5]),
        (0, vec![1, 2, 3, 4, 5]),
    ] {
        let got = l.uids(&ListOptions::at(2).first(first)).unwrap();
        assert_eq!(got.uids(), expected, "first: {first}");
    }
}

#[test]
fn test_uids_are_the_reference_postings_alone() {
    let mut l = List::empty(key("uids-refs", 1));
    add(&mut l, 1, &set_uid(3));
    add(&mut l, 1, &set_value("literal"));
    l.commit_mutation(2);
    assert_eq!(l.uids(&ListOptions::at(2)).unwrap().uids(), vec![3]);
    let mut seen = Vec::new();
    l.postings(&ListOptions::at(2), |p| {
        seen.push(p.uid());
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .unwrap();
    assert_eq!(seen, vec![3], "postings skips the value posting too");
}

#[test]
fn test_postings_stop_where_the_callback_breaks_and_pass_its_error() {
    let mut l = List::empty(key("uids-postings", 1));
    (1..=4).for_each(|u| add(&mut l, 1, &with_facet(set_uid(u), "f")));
    l.commit_mutation(2);
    let mut seen = Vec::new();
    l.postings(&ListOptions::at(2).after(1), |p| {
        seen.push((p.uid(), p.facets()[0].key().to_string()));
        Ok::<_, PostingError>(if p.uid() == 3 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        })
    })
    .unwrap();
    assert_eq!(seen, vec![(2, "f".to_string()), (3, "f".to_string())]);
    let error = l
        .postings(
            &ListOptions::at(2),
            |_| -> Result<ControlFlow<()>, PostingError> { Err(PostingError::TsTooOld()) },
        )
        .unwrap_err();
    assert_eq!(error.0, PostingErrorEnum::TsTooOld);
}

/// A reference uid that equals a literal's fingerprint in one quad list: the identities differ, (uid, "") against
/// (uid, encoding), so the two postings coexist in a delta.
fn coincident() -> List {
    let mut l = List::empty(key("uids-coincident", 1));
    let literal = Posting::default()
        .with_uid(5)
        .with_value(b"\x20\x01x".to_vec())
        .with_posting_type(PostingType::Value)
        .with_op(ultragraphdb_store_posting::SET);
    add(&mut l, 1, &set_uid(5));
    add(&mut l, 1, &literal);
    add(&mut l, 1, &set_uid(7));
    l.commit_mutation(2);
    l
}

#[test]
fn test_a_reference_coinciding_with_a_literals_fingerprint_is_one_uid() {
    let l = coincident();
    assert_eq!(l.uids(&ListOptions::at(2)).unwrap().uids(), vec![5, 7]);
    assert_eq!(l.length(2, 0), 3, "the reference, the literal and 7");
}

/// A rollup packs a uid that postings share once, and keeps beside the pack the postings it cannot carry; a bare
/// reference (no facets, no statement IDs) whose uid equals a literal's fingerprint in the same list is kept whole
/// beside the literal, so the read back holds both.
#[test]
fn test_a_rollup_keeps_a_reference_coinciding_with_a_literals_fingerprint() {
    let store = new_store();
    let l = coincident();
    let (rolled, _) = roll_up(&store, &l, u64::MAX, usize::MAX);
    let kept: Vec<(u64, PostingType)> = rolled
        .plist()
        .postings()
        .iter()
        .map(|p| (p.uid(), p.posting_type()))
        .collect();
    let (reference, value) = (PostingType::Ref, PostingType::Value);
    assert_eq!(kept, vec![(5, reference), (5, value)], "7 stays a bare uid");
    assert_eq!(rolled.length(u64::MAX, 0), 3);
    assert_eq!(
        rolled.uids(&ListOptions::at(u64::MAX)).unwrap().uids(),
        vec![5, 7]
    );
}

/// `uids` returns the uids of the list's reference postings on each of its three paths: iterating, the pack
/// intersected directly, and each uid looked up. A forward list holds references and literals since stage 4, and the
/// pack holds a literal's fingerprint as a uid, so the direct and lookup paths leave out a uid held by literals alone.
#[test]
fn test_uids_leave_out_a_literals_fingerprint_on_every_path() {
    let store = new_store();
    let mut l = List::empty(key("uids-mixed", 1));
    let literal = Posting::default()
        .with_uid(5)
        .with_value(b"\x20\x01x".to_vec())
        .with_posting_type(PostingType::Value)
        .with_op(ultragraphdb_store_posting::SET);
    add(&mut l, 1, &set_uid(3));
    add(&mut l, 1, &literal);
    l.commit_mutation(2);
    let with = || ListOptions::at(4).intersect(list_of(&[3, 5]));
    assert_eq!(l.uids(&with()).unwrap().uids(), vec![3], "iterating");
    let (rolled, _) = roll_up(&store, &l, u64::MAX, usize::MAX);
    assert_eq!(
        rolled.uids(&with()).unwrap().uids(),
        vec![3],
        "the pack directly"
    );
    let mut looked_up = rolled.clone();
    add(&mut looked_up, 3, &set_uid(9));
    looked_up.commit_mutation(4);
    assert_eq!(
        looked_up.uids(&with()).unwrap().uids(),
        vec![3],
        "each uid looked up"
    );
}

/// Where a reference and a literal share a uid, the uid is a reference's on every path, and a delete of the
/// reference leaves the literal and drops the uid.
#[test]
fn test_uids_keep_a_reference_coinciding_with_a_literals_fingerprint_on_every_path() {
    let store = new_store();
    let (rolled, _) = roll_up(&store, &coincident(), u64::MAX, usize::MAX);
    let with = |ts: u64| ListOptions::at(ts).intersect(list_of(&[5, 7]));
    assert_eq!(
        rolled.uids(&with(2)).unwrap().uids(),
        vec![5, 7],
        "the pack directly"
    );
    let mut looked_up = rolled.clone();
    add(&mut looked_up, 3, &set_uid(9));
    looked_up.commit_mutation(4);
    assert_eq!(
        looked_up.uids(&with(4)).unwrap().uids(),
        vec![5, 7],
        "each uid looked up"
    );
    add(&mut looked_up, 5, &del_uid(5));
    looked_up.commit_mutation(6);
    assert_eq!(
        looked_up.uids(&with(6)).unwrap().uids(),
        vec![7],
        "the reference deleted"
    );
    assert_eq!(looked_up.length(6, 0), 3, "the literal, 7 and 9");
    let iterated = looked_up
        .uids(&ListOptions::at(6).intersect(list_of(&[5, 7, 9])))
        .unwrap();
    assert_eq!(iterated.uids(), vec![7, 9], "iterating");
}
