/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::Posting;
use ultragraphdb_store_posting::List;

use crate::utils::helpers::{
    add, del_uid, delete_all, key, new_store, roll_up, set_uid, set_value, with_facet,
};

#[test]
fn test_lengths_and_postings_without_sorting() {
    let mut l = List::empty(key("len", 1));
    (1..=5).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    assert_eq!(l.get_length(3).unwrap(), 5);
    let (len, found) = l.get_posting_and_length(3, 0, 4);
    assert_eq!((len, found.map(|p| p.uid())), (5, Some(4)));
    let (len, found) = l.get_posting_and_length_no_sort(3, 0, 6, false).unwrap();
    assert_eq!((len, found), (5, None));
    let (len, found) = l.get_posting_and_length_no_sort(3, 0, 2, true).unwrap();
    assert_eq!((len, found.map(|p| p.uid())), (5, Some(2)));
    assert!(!l.is_empty(3, 0).unwrap());
    assert!(l.is_empty(3, 5).unwrap());
    assert!(l.is_empty(1, 0).unwrap());
}

#[test]
fn test_a_posting_is_found_in_the_deltas_or_else_in_the_immutable_layer() {
    let store = new_store();
    let mut l = List::empty(key("find", 1));
    add(&mut l, 1, &set_uid(2));
    add(&mut l, 1, &with_facet(set_uid(4), "f"));
    add(&mut l, 1, &set_uid(6));
    l.commit_mutation(2);
    let (mut l, _) = roll_up(&store, &l, u64::MAX, 2000);
    // A posting kept beside the pack comes back whole; a bare uid as a uid posting.
    let found = l.find_posting(2, 4).unwrap().unwrap();
    assert_eq!((found.uid(), found.facets()[0].key()), (4, "f"));
    assert_eq!(
        l.find_posting(2, 2).unwrap(),
        Some(Posting::default().with_uid(2))
    );
    assert_eq!(l.find_posting(2, 3).unwrap(), None, "between two uids");
    assert_eq!(l.find_posting(2, 7).unwrap(), None, "above the last uid");
    assert_eq!(l.find_posting(2, 0).unwrap(), None, "uid 0");
    // A delta's set is found there; a delta's delete hides the immutable posting.
    add(&mut l, 3, &with_facet(set_uid(6), "g"));
    add(&mut l, 3, &del_uid(2));
    l.commit_mutation(4);
    assert_eq!(
        l.find_posting(4, 6).unwrap().unwrap().facets()[0].key(),
        "g"
    );
    assert_eq!(l.find_posting(4, 2).unwrap(), None);
    assert_eq!(
        l.find_posting(3, 2).unwrap().map(|p| p.uid()),
        Some(2),
        "below the delete"
    );
    // A delete-all ends the search before the immutable layer.
    add(&mut l, 5, &delete_all());
    l.commit_mutation(6);
    assert_eq!(l.find_posting(6, 4).unwrap(), None);
    assert_eq!(l.find_posting(5, 4).unwrap().map(|p| p.uid()), Some(4));
}

#[test]
fn test_a_length_counts_the_immutable_layer_unless_a_delete_all_hides_it() {
    let store = new_store();
    let mut l = List::empty(key("get-length", 1));
    (1..=3).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let (mut l, _) = roll_up(&store, &l, u64::MAX, 2000);
    add(&mut l, 3, &set_uid(9));
    add(&mut l, 3, &del_uid(1));
    l.commit_mutation(4);
    // 3 in the pack, plus one set and minus one delete.
    assert_eq!(l.get_length(4).unwrap(), 3);
    assert_eq!(
        l.get_length(3).unwrap(),
        3,
        "the pack alone below the delta"
    );
    add(&mut l, 5, &delete_all());
    add(&mut l, 5, &set_uid(7));
    l.commit_mutation(6);
    assert_eq!(
        l.get_length(6).unwrap(),
        1,
        "only the set after the delete-all"
    );
    assert_eq!(l.length(6, 0), 1);
}

#[test]
fn test_a_length_and_a_posting_above_an_after_uid() {
    let mut l = List::empty(key("len-after", 1));
    (1..=5).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let (len, found) = l.get_posting_and_length(3, 2, 4);
    assert_eq!((len, found.map(|p| p.uid())), (3, Some(4)));
    let (len, found) = l.get_posting_and_length(3, 2, 1);
    assert_eq!((len, found), (3, None), "uid 1 is not above the after-uid");
    let (len, found) = l.get_posting_and_length_no_sort(3, 2, 1, false).unwrap();
    assert_eq!(
        (len, found.map(|p| p.uid())),
        (5, Some(1)),
        "without sorting, the after-uid is not applied"
    );
}

/// The value of the newest layer `static_value` picks, in the order its documentation gives.
fn static_of(l: &List, read_ts: u64) -> Option<Vec<u8>> {
    l.static_value(read_ts)
        .map(|pl| pl.postings()[0].value().to_vec())
}

#[test]
fn test_a_static_value_is_the_newest_layer_at_its_timestamp() {
    let store = new_store();
    let k = ultragraphdb_core_keys::QuadKey::VectorIndex {
        namespace: 0,
        graph: 0,
        predicate: 7,
        list: 1,
        node: 1,
    }
    .to_bytes();
    let mut l = List::empty(k);
    add(&mut l, 1, &set_value("a"));
    l.commit_mutation(2);
    let (mut l, _) = roll_up(&store, &l, u64::MAX, 2000);
    assert_eq!(
        static_of(&l, 2),
        Some(b"a".to_vec()),
        "the complete version"
    );
    add(&mut l, 3, &set_value("b"));
    l.commit_mutation(4);
    add(&mut l, 5, &set_value("c"));
    l.commit_mutation(6);
    assert_eq!(
        static_of(&l, 6),
        Some(b"c".to_vec()),
        "the newest delta, at the newest version"
    );
    assert_eq!(static_of(&l, 9), Some(b"c".to_vec()), "and above it");
    assert_eq!(
        static_of(&l, 5),
        Some(b"b".to_vec()),
        "the newest delta at or below the read"
    );
    assert_eq!(
        static_of(&l, 3),
        Some(b"a".to_vec()),
        "no delta at or below the read"
    );
    add(&mut l, 7, &set_value("d"));
    assert_eq!(
        static_of(&l, 7),
        Some(b"d".to_vec()),
        "the request's own write at its timestamp"
    );
    assert_eq!(
        static_of(&l, 8),
        Some(b"c".to_vec()),
        "not at another timestamp"
    );
}
