/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The merge of the deltas over the immutable layer, at the boundaries of the read timestamp, the uids and the
//! layers: an empty list, one posting, the complete version's own timestamp, the largest uid, a delete over a set
//! and a commit at the request's own read timestamp.

use std::ops::ControlFlow;
use ultragraphdb_tool_bdd::TestResult;

use ultragraphdb_core_codec::{Posting, PostingList, PostingType};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_store_posting::{
    List, ListOptions, PostingError, PostingErrorEnum, PostingIdentity, SET,
};

use crate::utils::helpers::{
    Failure, add, del_uid, delete_all, id, key, new_store, rid, rids, roll_up, set_uid, uids,
    with_facet,
};

/// The forward key of predicate 7 for subject 1 in the default graph of the root tenant.
fn known_key() -> TestResult<Vec<u8>> {
    Ok(QuadKey::Forward {
        tenant: 0,
        graph: Graph::Default,
        predicate: id(7)?,
        subject: id(1)?,
    }
    .to_bytes())
}

fn facets(list: &List, read_ts: u64) -> TestResult<Vec<(u128, String)>> {
    let mut out = Vec::new();
    list.iterate(read_ts, 0, |p| {
        out.push((
            p.uid(),
            p.facets()
                .first()
                .map(|f| f.key().to_string())
                .unwrap_or_default(),
        ));
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })?;
    Ok(out)
}

#[test]
fn test_an_empty_list_reads_empty_everywhere() -> TestResult {
    let l = List::empty(key("iter-empty", 1));
    for read_ts in [0, 1, u64::MAX] {
        assert!(uids(&l, read_ts, 0)?.is_empty());
        assert_eq!(l.length(read_ts, 0), 0);
        assert!(l.is_empty(read_ts, 0)?);
        assert_eq!(l.get_length(read_ts)?, 0);
        assert_eq!(l.find_posting(read_ts, rid(1))?, None);
        assert!(l.static_value(read_ts).is_none());
        assert!(l.uids(&ListOptions::at(read_ts))?.ids().is_empty());
        assert_eq!(l.pick_postings(read_ts), (0, Vec::new()));
    }
    assert_eq!(l.approx_len(), 0);
    Ok(())
}

#[test]
fn test_one_posting_is_read_from_its_commit_on() -> TestResult {
    let mut l = List::empty(key("iter-one", 1));
    add(&mut l, 4, &set_uid(9));
    l.commit_mutation(5);
    assert!(uids(&l, 4, 0)?.is_empty(), "not visible below its commit");
    assert_eq!(uids(&l, 5, 0)?, rids(&[9]));
    assert_eq!(uids(&l, 5, rid(8))?, rids(&[9]));
    assert!(
        uids(&l, 5, rid(9))?.is_empty(),
        "the after-uid itself is excluded"
    );
    assert_eq!((l.length(5, 0), l.get_length(5)?), (1, 1));
    assert_eq!((l.min_ts(), l.max_ts()), (0, 5));
    Ok(())
}

#[test]
fn test_a_read_at_the_complete_version_goes_ahead_and_one_below_is_refused() -> TestResult {
    let l = List::new(known_key()?, PostingList::default(), 5);
    assert!(uids(&l, 5, 0)?.is_empty());
    let error = l
        .iterate(4, 0, |_| Ok::<_, PostingError>(ControlFlow::Continue(())))
        .failure("an iteration below the complete version")?;
    assert_eq!(
        error.0,
        PostingErrorEnum::ReadBelowMinTs {
            read_ts: 4,
            min_ts: 5,
            key: known_key()?
        }
    );
    // Go's errors.Errorf("readTs: %d less than minTs: %d for key: %q", 4, 5, key): %q quotes each byte as \xNN but
    // 0x07, \a, written by hand over the key's 57 bytes.
    assert_eq!(
        error.to_string(),
        "readTs: 4 less than minTs: 5 for key: \"\\x10\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x01\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\a\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x01\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x01\\x00\\x00\\x00\\x00\\x00\\x00\""
    );
    assert_eq!(
        l.length(4, 0),
        -1,
        "Dgraph's Length reports a failed iteration as -1"
    );
    assert_eq!(l.get_posting_and_length(4, 0, rid(1)), (-1, None));
    Ok(())
}

#[test]
fn test_a_delete_all_below_the_complete_version_is_refused_and_one_at_it_hides_the_list()
-> TestResult {
    let store = new_store();
    let mut l = List::empty(key("iter-delete-below", 1));
    add(&mut l, 1, &set_uid(3));
    l.commit_mutation(2);
    let (rolled, _) = roll_up(&store, &l, 4, usize::MAX)?;
    assert_eq!(
        rolled.min_ts(),
        3,
        "a rollup at 4 writes one above the commit at 2"
    );

    let below = rolled.committed(2, PostingList::default().with_postings(vec![delete_all()]));
    let error = below
        .is_empty(5, 0)
        .failure("the emptiness test below the complete version")?;
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
    let at = rolled.committed(3, PostingList::default().with_postings(vec![delete_all()]));
    assert!(uids(&at, 3, 0)?.is_empty());
    assert_eq!(at.get_length(3)?, 0);
    assert_eq!(uids(&rolled, 3, 0)?, rids(&[3]));
    Ok(())
}

#[test]
fn test_only_the_newest_version_of_a_posting_counts() -> TestResult {
    let mut l = List::empty(key("iter-newest", 1));
    add(&mut l, 1, &with_facet(set_uid(5), "old"));
    l.commit_mutation(2);
    add(&mut l, 3, &with_facet(set_uid(5), "new"));
    l.commit_mutation(4);
    assert_eq!(facets(&l, 3)?, vec![(rid(5), "old".to_string())]);
    assert_eq!(facets(&l, 4)?, vec![(rid(5), "new".to_string())]);
    assert_eq!(l.length(4, 0), 1);
    Ok(())
}

#[test]
fn test_a_delete_hides_a_set_of_the_immutable_layer_and_a_later_set_restores_it() -> TestResult {
    let store = new_store();
    let mut l = List::empty(key("iter-delete-over-set", 1));
    [1, 2, 3].iter().for_each(|&u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let (mut l, _) = roll_up(&store, &l, u64::MAX, usize::MAX)?;
    add(&mut l, 3, &del_uid(2));
    add(&mut l, 3, &del_uid(7));
    l.commit_mutation(4);
    assert_eq!(uids(&l, 4, 0)?, rids(&[1, 3]));
    assert_eq!(l.length(4, 0), 2);
    assert_eq!(l.find_posting(4, rid(2))?, None);
    assert_eq!(
        uids(&l, 3, 0)?,
        rids(&[1, 2, 3]),
        "the delete is not visible below its commit"
    );
    add(&mut l, 5, &set_uid(2));
    l.commit_mutation(6);
    assert_eq!(uids(&l, 6, 0)?, rids(&[1, 2, 3]));
    Ok(())
}

/// Deviation: postings of one uid are ordered by their layer, the request's own newest, where Dgraph orders them by
/// an effective timestamp that ties between a commit at the read timestamp and the request's own write.
#[test]
fn test_a_requests_own_write_is_newer_than_a_commit_at_its_read_timestamp() -> TestResult {
    let mut l = List::empty(key("iter-tie", 1));
    add(&mut l, 4, &set_uid(3));
    l.commit_mutation(5);
    add(&mut l, 5, &del_uid(3));
    assert!(
        uids(&l, 5, 0)?.is_empty(),
        "the request deleted the uid committed at its read timestamp"
    );
    let mut other = List::empty(key("iter-tie", 2));
    add(&mut other, 4, &del_uid(3));
    other.commit_mutation(5);
    add(&mut other, 5, &set_uid(3));
    assert_eq!(uids(&other, 5, 0)?, rids(&[3]));
    Ok(())
}

#[test]
fn test_the_largest_sequence_is_read_from_the_deltas_and_from_the_pack() -> TestResult {
    let store = new_store();
    let mut l = List::empty(key("iter-max", 1));
    add(&mut l, 1, &set_uid(1));
    add(&mut l, 1, &set_uid(u64::MAX));
    l.commit_mutation(2);
    assert_eq!(uids(&l, 2, 0)?, rids(&[1, u64::MAX]));
    assert_eq!(uids(&l, 2, rid(u64::MAX) - 1)?, rids(&[u64::MAX]));
    let (rolled, _) = roll_up(&store, &l, u64::MAX, usize::MAX)?;
    assert_eq!(uids(&rolled, 2, 0)?, rids(&[1, u64::MAX]));
    assert_eq!(uids(&rolled, 2, rid(u64::MAX) - 1)?, rids(&[u64::MAX]));
    assert!(uids(&rolled, 2, rid(u64::MAX))?.is_empty());
    assert!(uids(&rolled, 2, u128::MAX)?.is_empty());
    assert_eq!(
        rolled.find_posting(2, rid(u64::MAX))?.map(|p| p.uid()),
        Some(rid(u64::MAX))
    );
    Ok(())
}

#[test]
fn test_iteration_stops_where_the_callback_breaks_and_passes_its_error() -> TestResult {
    let mut l = List::empty(key("iter-stop", 1));
    (1..=5).for_each(|u| add(&mut l, 1, &set_uid(u)));
    l.commit_mutation(2);
    let mut seen = Vec::new();
    l.iterate(2, 0, |p| {
        seen.push(p.uid());
        Ok::<_, PostingError>(if p.uid() == rid(2) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        })
    })?;
    assert_eq!(seen, rids(&[1, 2]));
    let error = l
        .iterate(2, 0, |_| -> Result<ControlFlow<()>, PostingError> {
            Err(PostingError::ts_too_old())
        })
        .failure("the callback's refusal")?;
    assert_eq!(error.0, PostingErrorEnum::TsTooOld);
    Ok(())
}

/// Iterating reports at trace level when it read fewer postings than it skipped as deleted: rollups lag behind the
/// deletes. One set and two deletes: 3 postings, 66 percent deleted (2 × 100 / 3, rounded down).
#[test]
fn test_a_read_of_mostly_deleted_postings_is_reported() -> TestResult {
    let capture = ultragraphdb_store_sim::LogCapture::new();
    capture.capture(|| -> TestResult {
        let mut l = List::empty(key("iter-deleted", 1));
        add(&mut l, 1, &set_uid(1));
        add(&mut l, 1, &del_uid(2));
        add(&mut l, 1, &del_uid(3));
        l.commit_mutation(2);
        assert_eq!(uids(&l, 2, 0)?, rids(&[1]));
        let mut even = List::empty(key("iter-deleted", 2));
        add(&mut even, 1, &set_uid(1));
        add(&mut even, 1, &del_uid(2));
        even.commit_mutation(2);
        assert_eq!(uids(&even, 2, 0)?, rids(&[1]));
        Ok(())
    })?;
    let events = capture.events();
    assert_eq!(
        events.len(),
        1,
        "as many read as deleted is not reported: {events:?}"
    );
    assert_eq!(events[0].level(), tracing::Level::TRACE);
    assert_eq!(events[0].field("total"), Some("3"));
    assert_eq!(events[0].field("percent_deleted"), Some("66"));
    Ok(())
}

/// A property over a split list: a read after any uid, part starts and part ends included, yields exactly the uids
/// above it, and each uid is found where it is held.
#[test]
fn test_a_read_after_every_uid_of_a_split_list_yields_the_uids_above_it() -> TestResult {
    let store = new_store();
    let mut l = List::empty(key("iter-sweep", 1));
    // Odd uids, so that a read can start between two held uids as well as at one.
    (1..=1200u64).for_each(|i| add(&mut l, 1, &set_uid(2 * i - 1)));
    l.commit_mutation(2);
    let (l, _) = roll_up(&store, &l, u64::MAX, 300)?;
    let splits = l.part_splits();
    assert!(splits.len() > 3, "{splits:?}");
    for after in 0..=2400u64 {
        let expected: Vec<u128> = (after / 2 + 1..=1200)
            .map(|i| rid(2 * i - 1))
            .filter(|&u| u > rid(after))
            .collect();
        // An after-key of 0 reads from the start.
        let after_key = if after == 0 { 0 } else { rid(after) };
        let got = uids(&l, 2, after_key)?;
        assert!(
            got == expected,
            "after {after}: first {:?}, {} uids",
            got.first(),
            got.len()
        );
        let held = after % 2 == 1;
        assert_eq!(
            l.find_posting(2, after_key)?.is_some(),
            held && after > 0,
            "uid {after}"
        );
    }
    Ok(())
}

/// A literal whose fingerprint is 0 (one in 2^64) is keyed 0, a key like any other: it is iterated, and so are the
/// postings after it, where Go's uid 0 marked an exhausted iterator.
#[test]
fn test_a_posting_keyed_0_is_iterated_with_the_postings_after_it() -> TestResult {
    let mut l = List::empty(key("iter-zero", 1));
    add(
        &mut l,
        1,
        &ultragraphdb_core_codec::Posting::default()
            .with_uid(0)
            .with_value(b"v".to_vec())
            .with_posting_type(ultragraphdb_core_codec::PostingType::Value)
            .with_op(ultragraphdb_store_posting::SET),
    );
    l.commit_mutation(2);
    assert_eq!(uids(&l, 2, 0)?, vec![0]);
    add(&mut l, 3, &set_uid(4));
    l.commit_mutation(4);
    let mut expected = vec![0];
    expected.extend(rids(&[4]));
    assert_eq!(uids(&l, 4, 0)?, expected);
    // Rolled up, the literal is the immutable layer's, before the pack's ID.
    let (rolled, _) = roll_up(&new_store(), &l, u64::MAX, usize::MAX)?;
    assert_eq!(uids(&rolled, 4, 0)?, expected);
    Ok(())
}

/// A value posting keyed `uid` holding `value`, as two literals whose fingerprints collide share a uid.
fn shared(uid: u128, value: &str) -> Posting {
    Posting::default()
        .with_uid(uid)
        .with_value(value.as_bytes().to_vec())
        .with_posting_type(PostingType::Value)
        .with_op(SET)
}

/// The identity of each posting `list` yields at `read_ts` after `after`.
fn identities(
    list: &List,
    read_ts: u64,
    after: Option<&PostingIdentity>,
) -> TestResult<Vec<PostingIdentity>> {
    let mut out = Vec::new();
    list.iterate_after(read_ts, after, |p| {
        let mut identity = PostingIdentity::default();
        list.identity_into(p, &mut identity);
        out.push(identity);
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })?;
    Ok(out)
}

/// A read resumed after each posting of `list` at `read_ts` yields exactly the postings after it: none lost, none repeated.
fn resumes_after_each(list: &List, read_ts: u64, expected: usize) -> TestResult {
    let whole = identities(list, read_ts, None)?;
    assert_eq!(whole.len(), expected, "{whole:?}");
    for (at, identity) in whole.iter().enumerate() {
        let rest = whole.get(at + 1..).unwrap_or_default();
        assert_eq!(
            identities(list, read_ts, Some(identity))?,
            rest,
            "after {identity:?}"
        );
    }
    Ok(())
}

/// An identity included reads the posting itself when the list holds it, and otherwise the next one in the list's order; a walk's
/// noting a posting makes the position exclusive again.
#[test]
fn test_a_resume_at_an_included_identity_reads_it_when_the_list_holds_it() -> TestResult {
    let mut l = List::empty(key("iter-at", 1));
    add(&mut l, 1, &set_uid(4));
    add(&mut l, 1, &shared(9, "low"));
    add(&mut l, 1, &shared(9, "high"));
    l.commit_mutation(2);
    let whole = identities(&l, 2, None)?;
    for (at, identity) in whole.iter().enumerate() {
        let included = identity.clone().included();
        assert!(included.is_included());
        assert_eq!(
            identities(&l, 2, Some(&included))?,
            whole.get(at..).unwrap_or_default(),
            "at {identity:?}"
        );
    }
    // Between the two postings of one uid, and between two uids: the list holds neither position.
    let between = PostingIdentity::new(9, b"ice".to_vec()).included();
    assert_eq!(
        identities(&l, 2, Some(&between))?,
        whole.get(1..).unwrap_or_default()
    );
    let before = PostingIdentity::new(rid(4) + 1, Vec::new()).included();
    assert_eq!(
        identities(&l, 2, Some(&before))?,
        whole
            .iter()
            .filter(|identity| identity.uid() > rid(4))
            .cloned()
            .collect::<Vec<_>>()
    );
    let mut noted = PostingIdentity::new(1, Vec::new()).included();
    l.iterate(2, 0, |p| {
        l.identity_into(p, &mut noted);
        Ok::<_, PostingError>(ControlFlow::Break(()))
    })?;
    assert!(!noted.is_included());
    Ok(())
}

#[test]
fn test_a_resume_between_two_postings_of_one_uid_loses_neither_and_repeats_neither() -> TestResult {
    let mut l = List::empty(key("iter-after", 1));
    add(&mut l, 1, &shared(9, "low"));
    add(&mut l, 1, &shared(9, "high"));
    add(&mut l, 1, &set_uid(4));
    l.commit_mutation(2);
    resumes_after_each(&l, 2, 3)?;
    let whole = identities(&l, 2, None)?;
    let high = PostingIdentity::new(9, b"high".to_vec());
    assert_eq!(whole.first(), Some(&high), "ordered by uid, then value");
    let low = identities(&l, 2, Some(&high))?;
    assert_eq!(
        low.first(),
        Some(&PostingIdentity::new(9, b"low".to_vec())),
        "the posting sharing the uid is read"
    );
    Ok(())
}

#[test]
fn test_a_resume_reads_a_list_spanning_its_immutable_layer_and_its_deltas() -> TestResult {
    let mut l = List::empty(key("iter-after", 2));
    add(&mut l, 1, &shared(9, "b"));
    add(&mut l, 1, &shared(9, "d"));
    add(&mut l, 1, &set_uid(4));
    add(&mut l, 1, &set_uid(6));
    l.commit_mutation(2);
    let (mut rolled, _) = roll_up(&new_store(), &l, u64::MAX, usize::MAX)?;
    // Above the complete version: between the two of one uid, past them, and deletes of an immutable posting.
    add(&mut rolled, 3, &shared(9, "c"));
    add(&mut rolled, 3, &set_uid(5));
    add(&mut rolled, 3, &del_uid(6));
    rolled.commit_mutation(4);
    resumes_after_each(&rolled, 4, 5)?;
    let values: Vec<Vec<u8>> = identities(&rolled, 4, None)?
        .iter()
        .take(3)
        .map(|i| i.value().to_vec())
        .collect();
    assert_eq!(values, vec![b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]);
    // At the complete version the deltas are not read.
    resumes_after_each(&rolled, 2, 4)?;
    Ok(())
}

#[test]
fn test_a_resume_after_a_posting_keyed_0_or_1_starts_at_it() -> TestResult {
    let mut l = List::empty(key("iter-after", 3));
    for (uid, value) in [(0, "a"), (0, "b"), (1, "c"), (1, "d")] {
        add(&mut l, 1, &shared(uid, value));
    }
    l.commit_mutation(2);
    resumes_after_each(&l, 2, 4)
}

#[test]
fn test_a_resume_after_a_posting_since_deleted_reads_what_follows_it() -> TestResult {
    let mut l = List::empty(key("iter-after", 4));
    add(&mut l, 1, &set_uid(4));
    add(&mut l, 1, &set_uid(6));
    l.commit_mutation(2);
    add(&mut l, 3, &del_uid(4));
    l.commit_mutation(4);
    let gone = PostingIdentity::new(rid(4), Vec::new());
    assert_eq!(
        identities(&l, 4, Some(&gone))?,
        vec![PostingIdentity::new(rid(6), Vec::new())]
    );
    Ok(())
}

#[test]
fn test_a_resume_across_a_split_list_s_parts_reads_each_posting_once() -> TestResult {
    let store = new_store();
    let mut l = List::empty(key("iter-after", 5));
    (1..=1200u64).for_each(|i| add(&mut l, 1, &set_uid(i)));
    l.commit_mutation(2);
    let (l, _) = roll_up(&store, &l, u64::MAX, 300)?;
    assert!(l.part_splits().len() > 1, "{:?}", l.part_splits());
    let whole = identities(&l, 2, None)?;
    assert_eq!(whole.len(), 1200);
    for (at, identity) in whole.iter().enumerate().step_by(37) {
        let rest = whole.get(at + 1..).unwrap_or_default();
        assert_eq!(identities(&l, 2, Some(identity))?, rest, "after {at}");
    }
    Ok(())
}

/// A list that does not compare values, the vector index's, tells its postings apart by uid alone.
#[test]
fn test_an_identity_in_a_list_comparing_no_values_is_its_uid() -> TestResult {
    let vector_key = QuadKey::VectorIndex {
        tenant: 0,
        graph: Graph::Default,
        predicate: id(7)?,
        list: 1,
        node: id(1)?,
    }
    .to_bytes();
    let mut l = List::empty(vector_key);
    add(&mut l, 1, &shared(9, "held"));
    l.commit_mutation(2);
    assert_eq!(
        identities(&l, 2, None)?,
        vec![PostingIdentity::new(9, Vec::new())]
    );
    Ok(())
}
