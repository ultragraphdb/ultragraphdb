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
use ultragraphdb_tool_bdd::{Context, TestResult};

use ultragraphdb_core_codec::{List as UidList, Posting, PostingList, PostingType, decode};
use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING, List, ListOptions, MAX_LIST_SIZE, PostingError,
    bin_split, from_backup_posting_list, marshal, split_key_of, unmarshal_complete,
    unmarshal_delta,
};
use ultragraphdb_store_sim::SimStore;
use xxhash_rust::xxh3::xxh3_64;

use crate::utils::helpers::{
    add, check_uids, del_uid, del_value, delete_all, id, ids, key, new_store, read_back, rid, rids,
    roll_up, set_uid, set_value, uids, with_facet, write_entries,
};

/// The key of the one value of a single-value list: the largest fingerprint.
const SINGLE: u128 = u64::MAX as u128;

/// A vector-index list of `attr` for `node`: a list holding one value, which the next write replaces, as the vector
/// index's lists hold theirs.
fn single_key(attr: &str, node: u64) -> TestResult<Vec<u8>> {
    Ok(ultragraphdb_core_keys::QuadKey::VectorIndex {
        tenant: 0,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: crate::utils::helpers::predicate(attr)?,
        list: 1,
        node: id(node)?,
    }
    .to_bytes())
}

/// A set of the one value of a single-value list.
fn set_single(value: &str) -> Posting {
    set_value(value).with_uid(SINGLE)
}

/// A delete of the one value of a single-value list.
fn del_single(value: &str) -> Posting {
    del_value(value).with_uid(SINGLE)
}

fn first(list: &List, read_ts: u64) -> TestResult<Posting> {
    let mut out = Posting::default();
    list.iterate(read_ts, 0, |p| {
        out = p.clone();
        Ok::<_, PostingError>(ControlFlow::Break(()))
    })
    .context("iterate")?;
    Ok(out)
}

fn check_value(list: &List, value: &str, read_ts: u64) -> TestResult {
    let p = first(list, read_ts)?;
    assert_eq!(p.uid(), SINGLE);
    assert_eq!(p.value(), value.as_bytes());
    Ok(())
}

fn count(list: &List, read_ts: u64) -> TestResult<usize> {
    Ok(uids(list, read_ts, 0)?.len())
}

/// The start timestamp of the first posting of `list`'s value at `read_ts`.
fn start_ts_at(list: &List, read_ts: u64) -> TestResult<u64> {
    Ok(list
        .static_value(read_ts)
        .context("a static value")?
        .postings()[0]
        .start_ts())
}

#[test]
fn test_get_single_posting() -> TestResult {
    let k = key("GetSinglePosting", 123);
    let create = |start_ts: u64, commit_ts: u64| {
        PostingList::default()
            .with_postings(vec![
                Posting::default()
                    .with_uid(rid(1))
                    .with_op(1)
                    .with_start_ts(start_ts)
                    .with_commit_ts(commit_ts),
            ])
            .with_commit_ts(start_ts)
    };
    let l = List::empty(k.clone());
    assert!(l.static_value(1).is_none());

    let versioned = List::new(k, create(1, 1), 0);
    assert_eq!(start_ts_at(&versioned, 1)?, 1);
    assert_eq!(start_ts_at(&versioned, 4)?, 1);

    let versioned = versioned.committed(3, create(2, 3));
    assert_eq!(start_ts_at(&versioned, 1)?, 1);
    assert_eq!(start_ts_at(&versioned, 3)?, 2);
    assert_eq!(start_ts_at(&versioned, 4)?, 2);

    // A transaction from 4 to 6.
    let versioned = versioned.committed(6, create(4, 6));
    assert_eq!(start_ts_at(&versioned, 5)?, 2);
    assert_eq!(start_ts_at(&versioned, 6)?, 4);
    Ok(())
}

#[test]
fn test_add_mutation() -> TestResult {
    let mut l = List::empty(key("name", 2));
    let edge = with_facet(set_uid(9), "testing");
    add(&mut l, 1, &edge);
    assert_eq!(uids(&l, 1, 0)?, rids(&[9]));
    assert_eq!(first(&l, 1)?.facets()[0].key(), "testing");

    add(&mut l, 1, &with_facet(set_uid(81), "testing"));
    assert_eq!(uids(&l, 1, 0)?, rids(&[9, 81]));
    add(&mut l, 1, &with_facet(set_uid(49), "testing"));
    assert_eq!(uids(&l, 1, 0)?, rids(&[9, 49, 81]));
    check_uids(&l, &[9, 49, 81], 1)?;

    // Delete an edge, add an edge, replace an edge.
    add(&mut l, 1, &del_uid(49));
    add(&mut l, 1, &with_facet(set_uid(69), "testing"));
    add(&mut l, 1, &with_facet(set_uid(9), "anti-testing"));
    l.commit_mutation(2);

    check_uids(&l, &[9, 69, 81], 3)?;
    assert_eq!(first(&l, 3)?.facets()[0].key(), "anti-testing");
    Ok(())
}

#[test]
fn test_add_mutation_value() -> TestResult {
    let store = new_store();
    let mut ol = read_back(&store, &single_key("value", 10)?)?;
    add(&mut ol, 1, &set_single("oh hey there"));
    check_value(&ol, "oh hey there", 1)?;

    // The same check after committing.
    ol.commit_mutation(2);
    check_value(&ol, "oh hey there", 3)?;

    // The value made it to the posting list. Changing it now.
    add(&mut ol, 3, &set_single("119"));
    check_value(&ol, "119", 3)?;
    Ok(())
}

#[test]
fn test_add_mutation_jchiu1() -> TestResult {
    let mut ol = List::empty(single_key("value", 12)?);
    add(&mut ol, 1, &set_single("cars"));
    ol.commit_mutation(2);
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "cars", 3)?;

    // Set the value to newcars, but do not commit yet.
    add(&mut ol, 3, &set_single("newcars"));
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "newcars", 3)?;

    add(&mut ol, 3, &set_single("someothercars"));
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "someothercars", 3)?;

    // Back to the committed value.
    add(&mut ol, 3, &set_single("cars"));
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "cars", 3)?;
    Ok(())
}

#[test]
fn test_add_mutation_del_set() -> TestResult {
    let k = single_key("value", 1534)?;
    // A delete-all, not committed, on one copy of the list.
    let mut ol = List::empty(k.clone());
    add(&mut ol, 1, &delete_all());

    // Another copy sets newcars and commits it.
    let mut ol1 = List::empty(k);
    add(&mut ol1, 2, &set_single("newcars"));
    ol1.commit_mutation(3);
    assert_eq!(ol1.length(3, 0), 1);
    check_value(&ol1, "newcars", 3)?;
    Ok(())
}

#[test]
fn test_add_mutation_del_read() -> TestResult {
    let mut ol = List::empty(single_key("value", 1543)?);
    add(&mut ol, 1, &set_single("newcars"));
    ol.commit_mutation(2);
    assert_eq!(ol.length(2, 0), 1);
    check_value(&ol, "newcars", 2)?;

    // A delete-all, not committed.
    add(&mut ol, 3, &delete_all());
    // Its own transaction sees zero length.
    assert_eq!(ol.length(3, 0), 0);

    ol.commit_mutation(5);
    // A read at 4 does not see the delete-all committed at 5.
    assert_eq!(ol.length(4, 0), 1);
    check_value(&ol, "newcars", 4)?;
    assert_eq!(ol.length(6, 0), 0);
    Ok(())
}

#[test]
fn test_add_mutation_jchiu2() -> TestResult {
    let mut ol = List::empty(single_key("value", 15)?);
    add(&mut ol, 1, &del_single("cars"));
    assert_eq!(ol.length(1, 0), 0);

    add(&mut ol, 1, &set_single("newcars"));
    assert_eq!(ol.length(1, 0), 1);
    check_value(&ol, "newcars", 1)?;
    Ok(())
}

#[test]
fn test_add_mutation_jchiu2_commit() -> TestResult {
    let mut ol = List::empty(single_key("value", 16)?);
    add(&mut ol, 1, &del_single("cars"));
    ol.commit_mutation(2);
    assert_eq!(ol.length(3, 0), 0);

    add(&mut ol, 3, &set_single("newcars"));
    ol.commit_mutation(4);
    assert_eq!(ol.length(5, 0), 1);
    check_value(&ol, "newcars", 5)?;
    Ok(())
}

#[test]
fn test_add_mutation_jchiu3() -> TestResult {
    let mut ol = List::empty(single_key("value", 29)?);
    add(&mut ol, 1, &set_single("cars"));
    ol.commit_mutation(2);
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "cars", 3)?;

    add(&mut ol, 3, &del_single("cars"));
    assert_eq!(ol.length(3, 0), 0);

    add(&mut ol, 3, &set_single("newcars"));
    assert_eq!(ol.length(3, 0), 1);
    check_value(&ol, "newcars", 3)?;

    add(&mut ol, 3, &del_single("newcars"));
    assert_eq!(ol.length(3, 0), 0);
    Ok(())
}

#[test]
fn test_add_mutation_mrjn1() -> TestResult {
    let mut ol = List::empty(single_key("value", 21)?);
    add(&mut ol, 1, &set_single("cars"));
    ol.commit_mutation(2);

    // Delete the committed value, twice, without committing.
    add(&mut ol, 3, &del_single("cars"));
    assert_eq!(ol.length(3, 0), 0);
    add(&mut ol, 3, &del_single("cars"));
    assert_eq!(ol.length(3, 0), 0);

    // Set it again.
    add(&mut ol, 3, &set_single("cars"));
    check_value(&ol, "cars", 3)?;

    // And delete it again.
    add(&mut ol, 3, &del_single("cars"));
    assert_eq!(ol.length(3, 0), 0);
    Ok(())
}

/// The untagged value of the newest version of `key` at or below `read_ts`, by a point get: what Go's
/// `GetSinglePosting` reads from Badger.
fn stored_value(store: &SimStore, key: &[u8], read_ts: u64) -> TestResult<Vec<u8>> {
    let item = ultragraphdb_traits_store::Store::get_at(store, key, read_ts)
        .context("get")?
        .context("a version")?;
    let plist = if item.meta() == BIT_COMPLETE_POSTING {
        unmarshal_complete(key, item.value()).context("complete")?
    } else {
        unmarshal_delta(key, item.value()).context("delta")?
    };
    Ok(plist
        .postings()
        .iter()
        .find(|p| p.uid() == SINGLE)
        .map(|p| p.value().to_vec())
        .unwrap_or_default())
}

#[test]
fn test_read_single_value() -> TestResult {
    let store = new_store();
    let k = single_key("value", 1240)?;
    let mut ol = read_back(&store, &k)?;
    let n = 10_000u64;
    for i in (2..=n).step_by(2) {
        add(&mut ol, i, &set_single(&format!("ho hey there{i}")));
        let delta = ol.delta().cloned().context("a delta")?;
        ol.commit_mutation(i + 1);
        write_entries(
            &store,
            vec![ultragraphdb_traits_store::Entry::put(
                k.clone(),
                i + 1,
                ultragraphdb_store_posting::BIT_DELTA_POSTING,
                marshal(&delta),
            )],
        )?;

        if i % 10 == 0 {
            // Frequent rollups, stored at an old timestamp.
            ol = roll_up(&store, &ol, i - 3, usize::MAX)?.0;
        }

        for j in 3.max(ol.min_ts())..i + 6 {
            let value = stored_value(&store, &k, j)?;
            check_value(&ol, std::str::from_utf8(&value).context("utf-8")?, j)?;
        }
    }
    Ok(())
}

#[test]
fn test_rollup_max_ts_is_set() -> TestResult {
    let store = new_store();
    let k = key("bal", 1333);
    let mut ol = read_back(&store, &k)?;
    let n = 1_000_000u64;
    for i in (2..=n).step_by(2) {
        add(&mut ol, i, &set_uid(i));
        ol.commit_mutation(i + 1);
        if i % 10_000 == 0 {
            let (version, entries) = ol
                .rollup_entries(n + i, usize::MAX)
                .context("rollup")?
                .context("output")?;
            assert!(entries.iter().all(|e| e.version() == i + 2));
            assert_eq!(version, i + 2);
            write_entries(&store, entries)?;
            ol = read_back(&store, &k)?;
        }
    }
    Ok(())
}

#[test]
fn test_million() -> TestResult {
    let store = new_store();
    let k = key("bal", 1331);
    let mut ol = read_back(&store, &k)?;
    let n = 1_000_000u64;
    let mut commits = 0usize;
    for i in (2..=n).step_by(2) {
        add(&mut ol, i, &set_uid(i));
        ol.commit_mutation(i + 1);
        if i % 10_000 == 0 {
            ol = roll_up(&store, &ol, u64::MAX, usize::MAX)?.0;
        }
        commits += 1;
    }
    let l = ol.uids(&ListOptions::at(n + 1)).context("uids")?;
    assert_eq!(l.ids().len(), commits);
    for (i, uid) in l.ids().iter().enumerate() {
        assert_eq!(*uid, id((i as u64 + 1) * 2)?);
    }
    Ok(())
}

#[test]
fn test_add_mutation_gru() -> TestResult {
    let store = new_store();
    let mut ol = read_back(&store, &key("question.tag", 0x01))?;
    add(&mut ol, 1, &set_uid(0x2b69_3088_816b_04b7));
    add(&mut ol, 1, &set_uid(0x29bf_442b_48a7_72e0));
    ol.commit_mutation(2);

    add(&mut ol, 3, &set_uid(0x38de_c821_d2ac_3a79));
    add(&mut ol, 3, &del_uid(0x2b69_3088_816b_04b7));
    ol.commit_mutation(4);
    assert_eq!(
        uids(&ol, 5, 0)?,
        rids(&[0x29bf_442b_48a7_72e0, 0x38de_c821_d2ac_3a79])
    );
    Ok(())
}

#[test]
fn test_add_mutation_gru2() -> TestResult {
    let store = new_store();
    let mut ol = read_back(&store, &key("question.tag", 0x100))?;
    add(&mut ol, 1, &set_uid(0x02));
    add(&mut ol, 1, &set_uid(0x03));
    ol.commit_mutation(2);

    // A new tag, and the two older ones deleted.
    add(&mut ol, 3, &del_uid(0x02));
    add(&mut ol, 3, &del_uid(0x03));
    add(&mut ol, 3, &set_uid(0x04));
    ol.commit_mutation(4);

    assert_eq!(uids(&ol, 5, 0)?, rids(&[0x04]));
    Ok(())
}

#[test]
fn test_add_and_del_mutation() -> TestResult {
    let store = new_store();
    let mut ol = read_back(&store, &key("dummy_key", 0x927))?;
    add(&mut ol, 1, &set_uid(0x02));
    ol.commit_mutation(2);

    add(&mut ol, 3, &del_uid(0x02));
    add(&mut ol, 3, &del_uid(0x02));
    ol.commit_mutation(4);
    check_uids(&ol, &[], 5)?;
    Ok(())
}

fn assert_lengths(ol: &List, read_ts: u64, all: i64, after_199: i64, top: u64) {
    assert_eq!(ol.length(read_ts, 0), all);
    assert_eq!(ol.length(read_ts, rid(199)), after_199);
    assert_eq!(ol.length(read_ts, rid(top)), 0);
}

#[test]
fn test_after_uid_count() -> TestResult {
    let store = new_store();
    let mut ol = read_back(&store, &single_key("value", 22)?)?;
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
    Ok(())
}

#[test]
fn test_after_uid_count2() -> TestResult {
    let store = new_store();
    let mut ol = read_back(&store, &single_key("value", 23)?)?;
    (100..300).for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_lengths(&ol, 1, 200, 100, 300);

    // Re-insert a quarter of the edges: the counts do not change.
    (100..300)
        .step_by(4)
        .for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_lengths(&ol, 1, 200, 100, 300);
    Ok(())
}

#[test]
fn test_delete() -> TestResult {
    let store = new_store();
    let mut ol = read_back(&store, &single_key("value", 25)?)?;
    (1..=30).for_each(|i| add(&mut ol, 1, &set_uid(i)));
    assert_eq!(ol.length(1, 0), 30);
    add(&mut ol, 1, &delete_all());
    assert_eq!(ol.length(1, 0), 0);
    ol.commit_mutation(2);
    assert_eq!(ol.length(3, 0), 0);
    Ok(())
}

#[test]
fn test_after_uid_count_with_commit() -> TestResult {
    let store = new_store();
    let mut ol = read_back(&store, &single_key("value", 26)?)?;
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
    Ok(())
}

/// A forward list's first part starts at 0, the smallest fingerprint, and the starts ascend.
fn verify_splits(splits: &[u128]) {
    assert_eq!(splits.first(), Some(&0));
    assert!(splits.windows(2).all(|w| w[1] > w[0]));
}

/// Go's `createMultiPartList`: `size` uids set one commit each, rolled up every 2,000 with a split size of 5,000
/// bytes, then rolled up at the maximum timestamp.
fn create_multi_part_list(
    store: &SimStore,
    name: &str,
    size: u64,
    facet: bool,
) -> TestResult<(List, usize)> {
    let max = 5000;
    let k = key(name, 1331);
    let mut ol = read_back(store, &k)?;
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
            ol = roll_up(store, &ol, u64::MAX, max)?.0;
        }
        commits += 1;
    }
    let (version, entries) = ol
        .rollup_entries(u64::MAX, max)
        .context("rollup")?
        .context("output")?;
    assert!(entries.iter().all(|e| e.version() == size + 1));
    assert_eq!(version, size + 1);
    write_entries(store, entries)?;
    let written = read_back(store, &k)?;
    assert!(written.plist().pack().is_none());
    assert!(written.plist().postings().is_empty());
    assert!(!written.plist().splits().is_empty());
    verify_splits(written.plist().splits());
    Ok((written, commits))
}

/// Go's `createAndDeleteMultiPartList`: `size` uids set, then each deleted, one commit each, rolled up every
/// 2,000 with a split size of 10,000 bytes.
fn create_and_delete_multi_part_list(store: &SimStore, size: u64) -> TestResult<(List, usize)> {
    let max = 10_000;
    let k = key("create-and-delete", 1331);
    let mut ol = read_back(store, &k)?;
    let mut commits = 0;
    for i in 1..=size {
        add(&mut ol, i, &set_uid(i));
        ol.commit_mutation(i + 1);
        if i % 2000 == 0 {
            ol = roll_up(store, &ol, u64::MAX, max)?.0;
        }
        commits += 1;
    }
    assert!(!ol.plist().splits().is_empty());
    verify_splits(ol.plist().splits());

    let base = size + 1;
    for i in 1..=size {
        add(&mut ol, base + i, &del_uid(i));
        ol.commit_mutation(base + i + 1);
        if i % 2000 == 0 {
            ol = roll_up(store, &ol, u64::MAX, max)?.0;
        }
        commits += 1;
    }
    assert!(ol.plist().splits().is_empty());
    Ok((ol, commits))
}

#[test]
fn test_large_plist_split() -> TestResult {
    let store = new_store();
    let k = key("large-plist-split", 1331);
    let mut ol = read_back(&store, &k)?;
    let big = vec![7u8; 300 << 10];
    for i in 1..=2u64 {
        let edge = with_facet(set_uid(i), &i.to_string()).with_value(big.clone());
        add(&mut ol, i, &edge);
        ol.commit_mutation(i + 1);
    }
    // Rolled up and not written, as in Go.
    let _rollup: Option<(u64, Vec<ultragraphdb_traits_store::Entry>)> = ol
        .rollup_entries(u64::MAX, MAX_LIST_SIZE)
        .context("rollup")?;

    let mut far_apart = read_back(&store, &k)?;
    // A block closes at the block size whatever the IDs' distance (Go's broke a block at each uid far apart), so the
    // list takes more references than one block holds, 300, each with a value of 2 KiB: 600 KiB over two blocks.
    let value = vec![9u8; 2 << 10];
    for i in 0..300u64 {
        let edge = with_facet(set_uid((i << 40) + 1), &i.to_string()).with_value(value.clone());
        add(&mut far_apart, i, &edge);
        far_apart.commit_mutation(i + 1);
    }
    let rolled = roll_up(&store, &far_apart, u64::MAX, MAX_LIST_SIZE)?.0;
    assert!(rolled.plist().postings().is_empty());
    assert!(!rolled.plist().splits().is_empty());
    verify_splits(rolled.plist().splits());
    Ok(())
}

#[test]
fn test_delete_star_multi_part_list() -> TestResult {
    let store = new_store();
    let num_edges = 10_000;
    let (mut list, _) = create_multi_part_list(&store, "delete-star", num_edges, false)?;
    assert_eq!(count(&list, u64::MAX)?, num_edges as usize);

    let read_ts = list.max_ts() + 1;
    add(&mut list, read_ts, &delete_all());
    list.commit_mutation(read_ts + 1);
    assert_eq!(count(&list, u64::MAX)?, 0);
    Ok(())
}

#[test]
fn test_multi_part_list_basic() -> TestResult {
    let store = new_store();
    let size = 100_000;
    let (ol, commits) = create_multi_part_list(&store, "basic", size, false)?;
    let l = ol.uids(&ListOptions::at(size + 1)).context("uids")?;
    assert_eq!(l.ids().len(), commits);
    for (i, uid) in l.ids().iter().enumerate() {
        assert_eq!(*uid, id(i as u64 + 1)?);
    }
    Ok(())
}

#[test]
fn test_bin_split() -> TestResult {
    let store = new_store();
    let size = 100_000u64;
    let k = key("bin-split", 1331);
    let mut ol = read_back(&store, &k)?;
    for i in 1..=size {
        add(&mut ol, i, &with_facet(set_uid(i), &i.to_string()));
        ol.commit_mutation(i + 1);
    }
    let (ol, version) = roll_up(&store, &ol, u64::MAX, usize::MAX)?;
    assert_eq!(version, size + 1);
    assert!(ol.plist().splits().is_empty());
    assert_eq!(ol.plist().postings().len(), size as usize);

    let verify =
        |plist: &PostingList, starts: [u128; 2], halves: &[PostingList; 2]| -> TestResult {
            let all = decode(plist.pack().context("a pack")?, 0);
            let mut low = decode(halves[0].pack().context("a pack")?, starts[0]);
            let high = decode(halves[1].pack().context("a pack")?, starts[1]);
            low.extend(high);
            assert_eq!(all, low);
            let mut postings = halves[0].postings().to_vec();
            postings.extend(halves[1].postings().to_vec());
            assert_eq!(plist.postings(), postings);
            let mid = halves[1].pack().context("a pack")?.blocks()[0]
                .first()
                .to_u128();
            assert_eq!(starts[1], mid);
            assert!(halves[0].postings().iter().all(|p| p.uid() < mid));
            assert!(halves[1].postings().iter().all(|p| p.uid() >= mid));
            Ok(())
        };
    let plist = ol.plist().clone();
    let (starts, halves) = bin_split(0, &plist)?;
    verify(&plist, starts, &halves)?;

    // The postings cut down to a third, and to none.
    let mut third = plist.clone();
    third.postings_mut().truncate(size as usize / 3);
    let (third_starts, third_halves) = bin_split(0, &third)?;
    verify(&third, third_starts, &third_halves)?;

    let mut none = plist;
    none.postings_mut().clear();
    let (none_starts, none_halves) = bin_split(0, &none)?;
    verify(&none, none_starts, &none_halves)?;
    Ok(())
}

#[test]
fn test_multi_part_list_iter_after_uid() -> TestResult {
    let store = new_store();
    let size = 100_000u64;
    let (ol, _) = create_multi_part_list(&store, "iter-after-uid", size, false)?;
    let visited = uids(&ol, size + 1, rid(50_000))?;
    assert_eq!(visited.len(), 50_000);
    for (i, uid) in visited.iter().enumerate() {
        assert_eq!(*uid, rid(50_000 + i as u64 + 1));
    }
    Ok(())
}

fn facet_keys(list: &List, read_ts: u64) -> TestResult<Vec<String>> {
    let mut facets = Vec::new();
    list.iterate(read_ts, 0, |p| {
        if let Some(f) = p.facets().first() {
            facets.push(f.key().to_string());
        }
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .context("iterate")?;
    Ok(facets)
}

#[test]
fn test_multi_part_list_with_postings() -> TestResult {
    let store = new_store();
    let size = 100_000u64;
    let (ol, commits) = create_multi_part_list(&store, "with-postings", size, true)?;
    let facets = facet_keys(&ol, size + 1)?;
    assert_eq!(facets.len(), commits);
    for (i, facet) in facets.iter().enumerate() {
        assert_eq!(*facet, (i + 1).to_string());
    }
    Ok(())
}

#[test]
fn test_multi_part_list_marshal() -> TestResult {
    let store = new_store();
    let size = 100_000u64;
    let (ol, _) = create_multi_part_list(&store, "marshal", size, false)?;

    let (_, mut entries) = ol
        .rollup_entries(u64::MAX, 5000)
        .context("rollup")?
        .context("output")?;
    assert_eq!(entries.len(), ol.plist().splits().len() + 1);
    entries.sort_by(|a, b| a.key().cmp(b.key()));
    for (i, start) in ol.plist().splits().iter().enumerate() {
        let part_key = split_key_of(entries[0].key(), *start).context("split key")?;
        assert_eq!(entries[i + 1].key(), part_key);
        assert_eq!(
            entries[i + 1].value(),
            Some(marshal(&ol.parts()[i]).as_slice())
        );
        assert_eq!(entries[i + 1].meta(), BIT_COMPLETE_POSTING);
        assert_eq!(entries[i + 1].version(), ol.min_ts());
    }
    Ok(())
}

#[test]
fn test_multi_part_list_write_to_disk() -> TestResult {
    let store = new_store();
    let size = 100_000u64;
    let (original, commits) = create_multi_part_list(&store, "write-to-disk", size, false)?;

    let (new_list, _) = roll_up(&store, &original, u64::MAX, 5000)?;
    let opt = ListOptions::at(size + 1);
    let original_uids = original.uids(&opt).context("uids")?;
    let new_uids = new_list.uids(&opt).context("uids")?;
    assert_eq!(original_uids.ids().len(), commits);
    assert_eq!(original_uids, new_uids);
    Ok(())
}

#[test]
fn test_multi_part_list_delete() -> TestResult {
    let store = new_store();
    let size = 10_000u64;
    let (ol, commits) = create_and_delete_multi_part_list(&store, size)?;
    assert_eq!(commits, size as usize * 2);
    assert_eq!(count(&ol, u64::MAX)?, 0);

    let (_, entries) = ol
        .rollup_entries(u64::MAX, 10_000)
        .context("rollup")?
        .context("output")?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].meta(), BIT_EMPTY_POSTING);
    assert_eq!(entries[0].version(), ol.min_ts());
    Ok(())
}

#[test]
fn test_multi_part_list_delete_and_add() -> TestResult {
    let store = new_store();
    let size = 100_000u64;
    let max = 5000;
    let k = key("delete-and-add", 1331);
    let mut ol = read_back(&store, &k)?;
    for i in 1..=size {
        add(&mut ol, i, &set_uid(i));
        ol.commit_mutation(i + 1);
        if i % 2000 == 0 {
            ol = roll_up(&store, &ol, u64::MAX, max)?.0;
        }
    }
    let all = ol.uids(&ListOptions::at(u64::MAX)).context("uids")?;
    assert_eq!(all.ids(), ids(&(1..=size).collect::<Vec<_>>())?);

    // Delete the first half.
    let base = size + 1;
    for i in 1..=size / 2 {
        add(&mut ol, base + i, &del_uid(i));
        ol.commit_mutation(base + i + 1);
        if i % 2000 == 0 {
            ol = roll_up(&store, &ol, u64::MAX, max)?.0;
        }
    }
    let (_, entries) = ol
        .rollup_entries(u64::MAX, max)
        .context("rollup")?
        .context("output")?;
    assert!(entries.iter().all(|e| e.version() == base + 1 + size / 2));
    write_entries(&store, entries)?;
    let mut halved = read_back(&store, &k)?;
    let half = halved.uids(&ListOptions::at(u64::MAX)).context("uids")?;
    assert_eq!(half.ids(), ids(&(size / 2 + 1..=size).collect::<Vec<_>>())?);

    // Add the deleted half back.
    let readd_base = 2 * size + 1;
    for i in 1..=size / 2 {
        add(&mut halved, readd_base + i, &set_uid(i));
        halved.commit_mutation(readd_base + i + 1);
        if i % 2000 == 0 {
            halved = roll_up(&store, &halved, u64::MAX, max)?.0;
        }
    }
    let rolled = roll_up(&store, &halved, u64::MAX, max)?.0;
    let restored = rolled.uids(&ListOptions::at(u64::MAX)).context("uids")?;
    assert_eq!(restored.ids(), ids(&(1..=size).collect::<Vec<_>>())?);
    Ok(())
}

#[test]
fn test_single_list_rollup() -> TestResult {
    let store = new_store();
    let size = 100_000u64;
    let (ol, commits) = create_multi_part_list(&store, "single-list-rollup", size, true)?;
    let facets = facet_keys(&ol, size + 1)?;
    assert_eq!(facets.len(), commits);
    for (i, facet) in facets.iter().enumerate() {
        assert_eq!(*facet, (i + 1).to_string());
    }

    let (bl, entry) = ol
        .to_backup_posting_list()
        .context("backup")?
        .context("output")?;
    assert_eq!(entry.meta(), BIT_COMPLETE_POSTING);
    let plist = from_backup_posting_list(&bl).context("from backup")?;
    assert!(plist.splits().is_empty());
    assert_eq!(
        decode(plist.pack().context("a pack")?, 0).len(),
        size as usize
    );
    Ok(())
}

#[test]
fn test_list_delete_marker() -> TestResult {
    let store = new_store();
    let mut ol = read_back(&store, &key("delete-marker", 13345))?;
    add(&mut ol, 1, &set_uid(1));
    ol.commit_mutation(2);
    add(&mut ol, 3, &delete_all());
    assert_eq!(ol.mutation().populate_delete_all(3), 3);
    Ok(())
}

/// Go's split tests: `size` uids with facets, one commit each and no rollup until the end, split at Dgraph's size.
fn recursively_split(store: &SimStore, name: &str, size: u64) -> TestResult<List> {
    let k = key(name, 1333);
    let mut ol = read_back(store, &k)?;
    for i in 1..=size {
        add(&mut ol, i, &with_facet(set_uid(i), &i.to_string()));
        ol.commit_mutation(i + 1);
    }
    let ol = roll_up(store, &ol, u64::MAX, MAX_LIST_SIZE)?.0;
    assert!(ol.plist().splits().len() > 2);
    Ok(ol)
}

#[test]
fn test_split_length() -> TestResult {
    let store = new_store();
    let size = 100_000u64;
    let ol = recursively_split(&store, "split-length", size)?;
    assert_eq!(
        ol.get_length(size + 10).context("length")?,
        i64::try_from(size)?
    );
    Ok(())
}

#[test]
fn test_recursive_splits() -> TestResult {
    let store = new_store();
    let size = 100_000u64;
    let ol = recursively_split(&store, "recursive-splits", size)?;
    let facets = facet_keys(&ol, size + 1)?;
    assert_eq!(facets.len(), size as usize);
    for (i, facet) in facets.iter().enumerate() {
        assert_eq!(*facet, (i + 1).to_string());
    }
    Ok(())
}

/// The IDs of `list` at `read_ts`.
fn uids_at(list: &List, read_ts: u64) -> TestResult<Vec<ultragraphdb_core_ids::SystemId>> {
    Ok(list
        .uids(&ListOptions::at(read_ts))
        .context("uids")?
        .into_ids())
}

#[test]
fn test_calculated_uids_respect_read_ts() -> TestResult {
    // Issue #9795: a read below the newest commit sees the list as of its timestamp. The port has no calculated-uid
    // cache, so the reads take the path Dgraph takes without it.
    let mut l = List::empty(key("calculatedUidsReadTs", 7));
    [3, 4, 10].iter().for_each(|&u| add(&mut l, 5, &set_uid(u)));
    l.commit_mutation(10);
    add(&mut l, 15, &del_uid(4));
    add(&mut l, 15, &set_uid(13));
    l.commit_mutation(20);

    assert_eq!(uids_at(&l, 20)?, ids(&[3, 10, 13])?);
    assert_eq!(uids_at(&l, 25)?, ids(&[3, 10, 13])?);
    assert_eq!(uids_at(&l, 15)?, ids(&[3, 4, 10])?);
    assert!(uids_at(&l, 5)?.is_empty());
    Ok(())
}

#[test]
fn test_calculated_uids_skipped_for_bounded_reads() -> TestResult {
    // Without the calculated-uid cache, Go's poisoned memo has no counterpart: every read returns the list's uids.
    let mut l = List::empty(key("calculatedUidsBoundedReads", 7));
    [2, 3, 4].iter().for_each(|&u| add(&mut l, 5, &set_uid(u)));
    l.commit_mutation(10);

    assert_eq!(uids_at(&l, 10)?, ids(&[2, 3, 4])?);
    let worker = l
        .uids(&ListOptions::at(10).first(i64::from(i32::MAX)))
        .context("uids")?;
    assert_eq!(worker.ids(), ids(&[2, 3, 4])?);
    let first = l.uids(&ListOptions::at(10).first(1)).context("uids")?;
    assert_eq!(first.ids(), ids(&[2])?);
    let negative = l.uids(&ListOptions::at(10).first(-1)).context("uids")?;
    assert_eq!(negative.ids(), ids(&[2, 3, 4])?);
    let intersect = l
        .uids(&ListOptions::at(10).intersect(UidList::new(ids(&[3])?)))
        .context("uids")?;
    assert_eq!(intersect.ids(), ids(&[3])?);
    Ok(())
}

#[test]
fn test_uids_does_not_apply_a_negative_first() -> TestResult {
    let build = |n: u64| {
        let mut l = List::empty(key(&format!("negativeFirstNotApplied{n}"), 7));
        [2, 4, 6, 8, 10]
            .iter()
            .for_each(|&u| add(&mut l, 5, &set_uid(u)));
        l.commit_mutation(10);
        l
    };
    let all = ids(&[2, 4, 6, 8, 10])?;
    for (n, first) in [-1i64, -2, -4, -5, -9, i64::from(i32::MIN), i64::MIN]
        .into_iter()
        .enumerate()
    {
        let got = build(n as u64)
            .uids(&ListOptions::at(10).first(first))
            .context("uids")?;
        assert_eq!(got.ids(), all, "first: {first}");
    }
    // The after-uid still applies.
    let got = build(99)
        .uids(&ListOptions::at(10).first(-2).after(rid(4)))
        .context("uids")?;
    assert_eq!(got.ids(), ids(&[6, 8, 10])?);
    Ok(())
}

#[test]
fn test_value_postings_are_kept_beside_the_uids() -> TestResult {
    // encode keeps a posting with facets or a value; a bare reference is only an ID in the pack, and a value's
    // fingerprint is never in it.
    let mut l = List::empty(key("kept", 1));
    add(&mut l, 1, &set_uid(5));
    add(&mut l, 1, &set_value("v"));
    l.commit_mutation(2);
    let out = l
        .rollup(3, true, MAX_LIST_SIZE)
        .context("rollup")?
        .context("output")?;
    assert_eq!(out.plist().postings().len(), 1);
    assert_eq!(out.plist().postings()[0].posting_type(), PostingType::Value);
    assert_eq!(decode(out.plist().pack().context("a pack")?, 0), ids(&[5])?);
    assert_eq!(out.plist().postings()[0].uid(), u128::from(xxh3_64(b"v")));
    Ok(())
}
