/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `List::uids` takes one of three paths: the pack intersected directly (no deltas, no parts), each uid of a short
//! intersection looked up, or the list iterated. Each is pinned here, with the read timestamp and the counts at their
//! boundaries.

use std::ops::ControlFlow;
use ultragraphdb_tool_bdd::TestResult;

use ultragraphdb_core_codec::{List as UidList, Posting, PostingType};
use ultragraphdb_store_posting::{List, ListOptions, PostingError, PostingErrorEnum};

use crate::utils::helpers::{
    Failure, add, del_uid, ids, key, new_store, rid, rids, roll_up, set_uid, set_value, with_facet,
};

/// The ID list of the references numbered `ns`.
fn list_of(ns: &[u64]) -> TestResult<UidList> {
    Ok(UidList::new(ids(ns)?))
}

/// A rolled-up list of the uids 2, 4, …, 600, complete at its rollup's version, with no deltas: 300 uids in two
/// blocks of 256, so its approximate length is 512 (2 × 256).
fn rolled_evens() -> TestResult<List> {
    let store = new_store();
    let mut l = List::empty(key("uids-evens", 1));
    (1..=300).for_each(|i| add(&mut l, 1, &set_uid(2 * i)));
    l.commit_mutation(2);
    Ok(roll_up(&store, &l, u64::MAX, usize::MAX)?.0)
}

#[test]
fn test_the_pack_is_intersected_directly_without_deltas() -> TestResult {
    let l = rolled_evens()?;
    assert!(l.mutation().is_empty() && l.part_splits().is_empty());
    assert_eq!(
        l.min_ts(),
        2,
        "the rollup at the maximum timestamp takes the commit's version"
    );
    let got = l.uids(&ListOptions::at(2).intersect(list_of(&[1, 4, 8, 9, 600, 601])?))?;
    assert_eq!(got.ids(), ids(&[4, 8, 600])?);
    // Go 1.27 probe of algo.IntersectCompressedWith over the same pack: [4 8]. The direct path seeks with SeekStart,
    // which keeps the after-uid itself, where iterating starts above it (below): a Dgraph inconsistency kept.
    let after = l.uids(
        &ListOptions::at(2)
            .intersect(list_of(&[1, 4, 8, 9])?)
            .after(rid(4)),
    )?;
    assert_eq!(after.ids(), ids(&[4, 8])?);
    let mut with_delta = l.clone();
    add(&mut with_delta, 2, &set_uid(1));
    with_delta.commit_mutation(3);
    // With a delta, four uids are fewer than the approximate length (1 + 512), so each is looked up, and the lookup
    // ignores the after-uid altogether, as Go's (list.go:1903-1918, findPostingWithItr per uid).
    let looked_up = with_delta.uids(
        &ListOptions::at(3)
            .intersect(list_of(&[1, 4, 8, 9])?)
            .after(rid(4)),
    )?;
    assert_eq!(looked_up.ids(), ids(&[1, 4, 8])?);
    Ok(())
}

#[test]
fn test_an_intersection_below_the_complete_version_is_too_old() -> TestResult {
    let l = rolled_evens()?;
    let error = l
        .uids(&ListOptions::at(1).intersect(list_of(&[4])?))
        .failure("an intersection below the complete version")?;
    assert_eq!(error.0, PostingErrorEnum::TsTooOld);
    assert_eq!(error.to_string(), "Transaction is too old");
    Ok(())
}

#[test]
fn test_a_short_intersection_looks_each_uid_up() -> TestResult {
    let mut l = rolled_evens()?;
    // A delta makes the direct path unavailable; two uids are fewer than the approximate length, 1 + 512.
    add(&mut l, 3, &del_uid(4));
    add(&mut l, 3, &set_uid(5));
    l.commit_mutation(4);
    assert_eq!(
        l.approx_len(),
        513,
        "one delta layer plus two blocks of 256"
    );
    let got = l.uids(&ListOptions::at(4).intersect(list_of(&[4, 5, 6, 7])?))?;
    assert_eq!(
        got.ids(),
        ids(&[5, 6])?,
        "4 deleted, 5 set, 6 held, 7 never held"
    );
    let before = l.uids(&ListOptions::at(3).intersect(list_of(&[4, 5, 6])?))?;
    assert_eq!(
        before.ids(),
        ids(&[4, 6])?,
        "the delta at 4 is not visible at 3"
    );
    Ok(())
}

#[test]
fn test_a_long_intersection_iterates_only_its_range() -> TestResult {
    // A list of deltas alone: its approximate length is its one layer, so an intersection of two or more uids
    // iterates, reading only from the intersection's first to its last uid.
    let mut l = List::empty(key("uids-range", 1));
    (1..=10).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    assert_eq!(l.approx_len(), 1);
    let got = l.uids(&ListOptions::at(2).intersect(list_of(&[3, 7, 12])?))?;
    assert_eq!(got.ids(), ids(&[3, 7])?);
    let after = l.uids(
        &ListOptions::at(2)
            .intersect(list_of(&[3, 7, 12])?)
            .after(rid(3)),
    )?;
    assert_eq!(
        after.ids(),
        ids(&[7])?,
        "iterating starts above the after-uid"
    );
    let first = l.uids(&ListOptions::at(2).intersect(list_of(&[3, 5, 7])?).first(2))?;
    assert_eq!(
        first.ids(),
        ids(&[3, 5])?,
        "the count applies after the intersection"
    );
    let empty = l.uids(&ListOptions::at(2).intersect(list_of(&[])?))?;
    assert!(
        empty.ids().is_empty(),
        "an empty intersection holds nothing"
    );
    Ok(())
}

#[test]
fn test_a_positive_count_at_and_around_the_list_length() -> TestResult {
    let mut l = List::empty(key("uids-first", 1));
    (1..=5).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    for (first, expected) in [
        (4, vec![1, 2, 3, 4]),
        (5, vec![1, 2, 3, 4, 5]),
        (6, vec![1, 2, 3, 4, 5]),
        (0, vec![1, 2, 3, 4, 5]),
    ] {
        let got = l.uids(&ListOptions::at(2).first(first))?;
        assert_eq!(got.ids(), ids(&expected)?, "first: {first}");
    }
    Ok(())
}

#[test]
fn test_uids_are_the_reference_postings_alone() -> TestResult {
    let mut l = List::empty(key("uids-refs", 1));
    add(&mut l, 1, &set_uid(3));
    add(&mut l, 1, &set_value("literal"));
    l.commit_mutation(2);
    assert_eq!(l.uids(&ListOptions::at(2))?.ids(), ids(&[3])?);
    let mut seen = Vec::new();
    l.postings(&ListOptions::at(2), |p| {
        seen.push(p.uid());
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })?;
    assert_eq!(seen, rids(&[3]), "postings skips the value posting too");
    Ok(())
}

#[test]
fn test_postings_stop_where_the_callback_breaks_and_pass_its_error() -> TestResult {
    let mut l = List::empty(key("uids-postings", 1));
    (1..=4).for_each(|u| add(&mut l, 1, &with_facet(set_uid(u), "f")));
    l.commit_mutation(2);
    let mut seen = Vec::new();
    l.postings(&ListOptions::at(2).after(rid(1)), |p| {
        seen.push((p.uid(), p.facets()[0].key().to_string()));
        Ok::<_, PostingError>(if p.uid() == rid(3) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        })
    })?;
    assert_eq!(
        seen,
        vec![(rid(2), "f".to_string()), (rid(3), "f".to_string())]
    );
    let error = l
        .postings(
            &ListOptions::at(2),
            |_| -> Result<ControlFlow<()>, PostingError> { Err(PostingError::ts_too_old()) },
        )
        .failure("the callback's refusal")?;
    assert_eq!(error.0, PostingErrorEnum::TsTooOld);
    Ok(())
}

/// A reference and a literal numbered alike in one quad list: the literal keyed by its fingerprint 5, the reference
/// by the ID numbered 5, which no fingerprint equals, so the two postings never share a key and the literal sorts
/// first.
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
fn test_a_reference_numbered_as_a_literals_fingerprint_is_one_id() -> TestResult {
    let l = coincident();
    assert_eq!(l.uids(&ListOptions::at(2))?.ids(), ids(&[5, 7])?);
    assert_eq!(l.length(2, 0), 3, "the reference, the literal and 7");
    Ok(())
}

/// A rollup packs the references' IDs and keeps beside the pack the postings it cannot carry: the literal, keyed by
/// its fingerprint, while the bare references are IDs in the pack alone.
#[test]
fn test_a_rollup_keeps_a_literal_beside_the_references_it_packs() -> TestResult {
    let store = new_store();
    let l = coincident();
    let (rolled, _) = roll_up(&store, &l, u64::MAX, usize::MAX)?;
    let kept: Vec<(u128, PostingType)> = rolled
        .plist()
        .postings()
        .iter()
        .map(|p| (p.uid(), p.posting_type()))
        .collect();
    assert_eq!(kept, vec![(5, PostingType::Value)], "5 and 7 stay bare IDs");
    assert_eq!(rolled.length(u64::MAX, 0), 3);
    assert_eq!(
        rolled.uids(&ListOptions::at(u64::MAX))?.ids(),
        ids(&[5, 7])?
    );
    Ok(())
}

/// `uids` returns the IDs of the list's reference postings on each of its three paths: iterating, the pack
/// intersected directly, and each ID looked up. A forward list holds references and literals since stage 4; the pack
/// holds references' IDs alone, so no path yields a literal, even one whose fingerprint shares its number with an ID
/// asked for.
#[test]
fn test_uids_leave_out_a_literals_fingerprint_on_every_path() -> TestResult {
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
    let set = list_of(&[3, 5])?;
    let with = || ListOptions::at(4).intersect(set.clone());
    assert_eq!(l.uids(&with())?.ids(), ids(&[3])?, "iterating");
    let (rolled, _) = roll_up(&store, &l, u64::MAX, usize::MAX)?;
    assert_eq!(rolled.uids(&with())?.ids(), ids(&[3])?, "the pack directly");
    let mut looked_up = rolled.clone();
    add(&mut looked_up, 3, &set_uid(9));
    looked_up.commit_mutation(4);
    assert_eq!(
        looked_up.uids(&with())?.ids(),
        ids(&[3])?,
        "each uid looked up"
    );
    Ok(())
}

/// Where a reference and a literal share a number, the ID is the reference's on every path, and a delete of the
/// reference leaves the literal and drops the ID.
#[test]
fn test_uids_keep_a_reference_coinciding_with_a_literals_fingerprint_on_every_path() -> TestResult {
    let store = new_store();
    let (rolled, _) = roll_up(&store, &coincident(), u64::MAX, usize::MAX)?;
    let set = list_of(&[5, 7])?;
    let with = |ts: u64| ListOptions::at(ts).intersect(set.clone());
    assert_eq!(
        rolled.uids(&with(2))?.ids(),
        ids(&[5, 7])?,
        "the pack directly"
    );
    let mut looked_up = rolled.clone();
    add(&mut looked_up, 3, &set_uid(9));
    looked_up.commit_mutation(4);
    assert_eq!(
        looked_up.uids(&with(4))?.ids(),
        ids(&[5, 7])?,
        "each uid looked up"
    );
    add(&mut looked_up, 5, &del_uid(5));
    looked_up.commit_mutation(6);
    assert_eq!(
        looked_up.uids(&with(6))?.ids(),
        ids(&[7])?,
        "the reference deleted"
    );
    assert_eq!(looked_up.length(6, 0), 3, "the literal, 7 and 9");
    let iterated = looked_up.uids(&ListOptions::at(6).intersect(list_of(&[5, 7, 9])?))?;
    assert_eq!(iterated.ids(), ids(&[7, 9])?, "iterating");
    Ok(())
}

/// A reference's key is a valid system ID; a reference keyed below 2^64, by a fingerprint, is corruption naming the
/// list's key, on the iterating path and in a rollup alike.
#[test]
fn test_a_reference_keyed_by_no_id_is_corruption() -> TestResult {
    let k = key("uids-no-id", 1);
    let mut l = List::empty(k.clone());
    add(
        &mut l,
        1,
        &Posting::default()
            .with_uid(5)
            .with_op(ultragraphdb_store_posting::SET),
    );
    l.commit_mutation(2);
    let expected = PostingErrorEnum::Store(ultragraphdb_error_store::StoreError(
        ultragraphdb_error_store::StoreErrorEnum::Corrupted {
            key: k.clone(),
            reason: "a reference keyed by no ID".to_string(),
        },
    ));
    let iterated = l
        .uids(&ListOptions::at(2))
        .failure("the IDs of a reference keyed by no ID")?;
    assert_eq!(iterated.0, expected);
    let rolled = l
        .rollup(2, true, usize::MAX)
        .failure("the rollup of a reference keyed by no ID")?;
    assert_eq!(rolled.0, expected);
    Ok(())
}
