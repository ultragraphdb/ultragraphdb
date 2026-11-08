/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{Posting, PostingType};
use ultragraphdb_store_posting::{List, SET};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::helpers::{
    add, del_uid, del_value, delete_all, id, key, new_store, rid, roll_up, set_uid, set_value,
    with_facet,
};

#[test]
fn test_lengths_and_postings_without_sorting() -> TestResult {
    let mut l = List::empty(key("len", 1));
    (1..=5).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    assert_eq!(l.get_length(3)?, 5);
    let (len, found) = l.get_posting_and_length(3, 0, rid(4));
    assert_eq!((len, found.map(|p| p.uid())), (5, Some(rid(4))));
    let (unsorted_len_6, unsorted_found_6) =
        l.get_posting_and_length_no_sort(3, 0, rid(6), false)?;
    assert_eq!((unsorted_len_6, unsorted_found_6), (5, None));
    let (unsorted_len_2, unsorted_found_2) =
        l.get_posting_and_length_no_sort(3, 0, rid(2), true)?;
    assert_eq!(
        (unsorted_len_2, unsorted_found_2.map(|p| p.uid())),
        (5, Some(rid(2)))
    );
    assert!(!l.is_empty(3, 0)?);
    assert!(l.is_empty(3, rid(5))?);
    assert!(l.is_empty(1, 0)?);
    Ok(())
}

#[test]
fn test_a_posting_is_found_in_the_deltas_or_else_in_the_immutable_layer() -> TestResult {
    let store = new_store();
    let mut l = List::empty(key("find", 1));
    add(&mut l, 1, &set_uid(2));
    add(&mut l, 1, &with_facet(set_uid(4), "f"));
    add(&mut l, 1, &set_uid(6));
    l.commit_mutation(2);
    let (mut l, _) = roll_up(&store, &l, u64::MAX, 2000)?;
    // A posting kept beside the pack comes back whole; a bare uid as a uid posting.
    let found = l.find_posting(2, rid(4))?.context("the posting")?;
    assert_eq!((found.uid(), found.facets()[0].key()), (rid(4), "f"));
    assert_eq!(
        l.find_posting(2, rid(2))?,
        Some(Posting::default().with_uid(rid(2)))
    );
    assert_eq!(l.find_posting(2, rid(3))?, None, "between two uids");
    assert_eq!(l.find_posting(2, rid(7))?, None, "above the last uid");
    assert_eq!(l.find_posting(2, 0)?, None, "key 0");
    // A delta's set is found there; a delta's delete hides the immutable posting.
    add(&mut l, 3, &with_facet(set_uid(6), "g"));
    add(&mut l, 3, &del_uid(2));
    l.commit_mutation(4);
    assert_eq!(
        l.find_posting(4, rid(6))?.context("the posting")?.facets()[0].key(),
        "g"
    );
    assert_eq!(l.find_posting(4, rid(2))?, None);
    assert_eq!(
        l.find_posting(3, rid(2))?.map(|p| p.uid()),
        Some(rid(2)),
        "below the delete"
    );
    // A delete-all ends the search before the immutable layer.
    add(&mut l, 5, &delete_all());
    l.commit_mutation(6);
    assert_eq!(l.find_posting(6, rid(4))?, None);
    assert_eq!(l.find_posting(5, rid(4))?.map(|p| p.uid()), Some(rid(4)));
    Ok(())
}

#[test]
fn test_a_posting_matching_is_found_by_its_uid_and_its_pick_in_either_layer() -> TestResult {
    let store = new_store();
    let mut l = List::empty(key("match", 1));
    let zero = Posting::default()
        .with_uid(0)
        .with_value(b"zero".to_vec())
        .with_posting_type(PostingType::Value)
        .with_op(SET);
    add(&mut l, 1, &zero);
    add(&mut l, 1, &set_uid(2));
    add(&mut l, 1, &set_value("a"));
    add(&mut l, 1, &set_uid(6));
    l.commit_mutation(2);
    let (mut l, _) = roll_up(&store, &l, u64::MAX, 2000)?;
    let a = set_value("a");
    let value_is = |value: &'static [u8]| move |p: &Posting| p.value() == value;
    // The immutable layer: the uid's posting that the pick takes, key 0 among them, and none the pick refuses.
    let of_a = l.find_posting_matching(2, a.uid(), value_is(b"a"))?;
    assert_eq!(of_a.map(|p| p.value().to_vec()), Some(b"a".to_vec()));
    assert_eq!(l.find_posting_matching(2, a.uid(), value_is(b"b"))?, None);
    let of_zero = l.find_posting_matching(2, 0, value_is(b"zero"))?;
    assert_eq!(of_zero.map(|p| p.uid()), Some(0), "key 0");
    let of_two = l.find_posting_matching(2, rid(2), |_| true)?;
    assert_eq!(of_two.map(|p| p.uid()), Some(rid(2)));
    assert_eq!(
        l.find_posting_matching(2, rid(3), |_| true)?,
        None,
        "between two uids"
    );
    assert_eq!(
        l.find_posting_matching(2, rid(7), |_| true)?,
        None,
        "above the last uid"
    );
    // A delta's delete of the value hides it; a delta's set is found there.
    add(&mut l, 3, &del_value("a"));
    add(&mut l, 3, &set_uid(8));
    l.commit_mutation(4);
    assert_eq!(l.find_posting_matching(4, a.uid(), value_is(b"a"))?, None);
    let below = l.find_posting_matching(3, a.uid(), value_is(b"a"))?;
    assert!(below.is_some(), "below the delete");
    let of_eight = l.find_posting_matching(4, rid(8), |_| true)?;
    assert_eq!(of_eight.map(|p| p.uid()), Some(rid(8)));
    Ok(())
}

/// The immutable layer's seek for uid 1 starts at the first key, key 0 among them: a posting below the uid is passed over, and
/// the uid's own is found.
#[test]
fn test_a_posting_matching_uid_one_is_found_past_key_zero() -> TestResult {
    let store = new_store();
    let mut l = List::empty(key("match", 2));
    let value_at = |uid: u128, value: &[u8]| {
        Posting::default()
            .with_uid(uid)
            .with_value(value.to_vec())
            .with_posting_type(PostingType::Value)
            .with_op(SET)
    };
    add(&mut l, 1, &value_at(0, b"zero"));
    add(&mut l, 1, &value_at(1, b"one"));
    l.commit_mutation(2);
    let (l, _) = roll_up(&store, &l, u64::MAX, 2000)?;
    let one = l.find_posting_matching(2, 1, |p| p.value() == b"one")?;
    assert_eq!(one.map(|p| p.uid()), Some(1));
    Ok(())
}

#[test]
fn test_a_length_counts_the_immutable_layer_unless_a_delete_all_hides_it() -> TestResult {
    let store = new_store();
    let mut l = List::empty(key("get-length", 1));
    (1..=3).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let (mut l, _) = roll_up(&store, &l, u64::MAX, 2000)?;
    add(&mut l, 3, &set_uid(9));
    add(&mut l, 3, &del_uid(1));
    l.commit_mutation(4);
    // 3 in the pack, plus one set and minus one delete.
    assert_eq!(l.get_length(4)?, 3);
    assert_eq!(l.get_length(3)?, 3, "the pack alone below the delta");
    add(&mut l, 5, &delete_all());
    add(&mut l, 5, &set_uid(7));
    l.commit_mutation(6);
    assert_eq!(l.get_length(6)?, 1, "only the set after the delete-all");
    assert_eq!(l.length(6, 0), 1);
    Ok(())
}

#[test]
fn test_a_length_and_a_posting_above_an_after_uid() -> TestResult {
    let mut l = List::empty(key("len-after", 1));
    (1..=5).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let (len, found) = l.get_posting_and_length(3, rid(2), rid(4));
    assert_eq!((len, found.map(|p| p.uid())), (3, Some(rid(4))));
    let (len_after_1, found_after_1) = l.get_posting_and_length(3, rid(2), rid(1));
    assert_eq!(
        (len_after_1, found_after_1),
        (3, None),
        "uid 1 is not above the after-uid"
    );
    let (unsorted_len, unsorted_found) =
        l.get_posting_and_length_no_sort(3, rid(2), rid(1), false)?;
    assert_eq!(
        (unsorted_len, unsorted_found.map(|p| p.uid())),
        (5, Some(rid(1))),
        "without sorting, the after-uid is not applied"
    );
    Ok(())
}

/// The value of the newest layer `static_value` picks, in the order its documentation gives.
fn static_of(l: &List, read_ts: u64) -> Option<Vec<u8>> {
    l.static_value(read_ts)
        .map(|pl| pl.postings()[0].value().to_vec())
}

#[test]
fn test_a_static_value_is_the_newest_layer_at_its_timestamp() -> TestResult {
    let store = new_store();
    let k = ultragraphdb_core_keys::QuadKey::VectorIndex {
        tenant: 0,
        graph: ultragraphdb_core_keys::Graph::Default,
        predicate: id(7)?,
        list: 1,
        node: id(1)?,
    }
    .to_bytes();
    let mut l = List::empty(k);
    add(&mut l, 1, &set_value("a"));
    l.commit_mutation(2);
    let (mut l, _) = roll_up(&store, &l, u64::MAX, 2000)?;
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
    Ok(())
}
