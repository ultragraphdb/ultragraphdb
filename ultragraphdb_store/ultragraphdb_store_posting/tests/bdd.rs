/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The posting lists' scenarios: every scenario of the specs the crate owns, run by `cucumber` over the features
//! under `features/`.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

use cucumber::{World, then, when};
use ultragraphdb_core_codec::PostingList;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::split_key_of;
use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING, List, ListsConfig, MAX_LIST_SIZE, PostingError,
    PostingErrorEnum, Writes, marshal,
};
use ultragraphdb_store_sim::{
    Corruption, Event, Fault, FaultConfig, RunOutcome, Seed, SimClock, SimContext, SimListCache,
    SimStore, Simulation, Stream,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_store::{Entry, ListCache, PersistMode, Store, StoreSnapshot};

/// The unit tests' helpers and the posting workload, shared by path.
pub mod utils {
    #[path = "helpers.rs"]
    pub mod helpers;
    #[path = "posting_workload.rs"]
    pub mod posting_workload;
}

use crate::utils::helpers::{
    Failure, ProbeLists, TestLists, add, commit, commit_at, commit_edges, commit_edges_at, del_uid,
    del_value, delete_all, key, lists_over, new_lists, new_store, probe_lists, read_back, roll_up,
    set_uid, set_value, uids, with_facet, write_delta, write_entries,
};
use crate::utils::posting_workload::PostingWorkload;

#[derive(Debug, Default, World)]
struct Steps {
    uids: Vec<u64>,
    second: Vec<u64>,
    list: Option<List>,
    versions: Vec<u64>,
    flags: Vec<bool>,
    error: Option<PostingError>,
    store: Option<SimStore>,
    key: Vec<u8>,
    texts: Vec<Vec<u8>>,
    outcomes: BTreeSet<u64>,
    reads: Vec<Vec<Vec<u64>>>,
}

fn friend() -> Vec<u8> {
    key("friend", 1)
}

fn name() -> Vec<u8> {
    key("name", 1)
}

/// The value `list` holds at `read_ts`, `None` when it holds none.
fn value_of(list: &List, read_ts: u64) -> TestResult<Option<Vec<u8>>> {
    let mut value = None;
    list.iterate(read_ts, 0, |p| {
        value = Some(p.value().to_vec());
        Ok::<_, PostingError>(std::ops::ControlFlow::Break(()))
    })
    .context("the list iterates")?;
    Ok(value)
}

// Posting lists: reads.

#[when("a list holds a delta committed at 6 and is read at 5")]
fn later_delta(w: &mut Steps) -> TestResult {
    let store = new_store();
    write_delta(&store, &friend(), 6, &[set_uid(9)])?;
    let lists = lists_over(store, ListsConfig::default());
    w.uids = uids(&lists.read(&friend(), 5)?, 5, 0)?;
    Ok(())
}

#[then("the result excludes the delta's postings")]
fn excludes_delta(w: &mut Steps) -> TestResult {
    assert!(w.uids.is_empty(), "{:?}", w.uids);
    Ok(())
}

#[when("a list holds a complete version at 4 and deltas at 5 and 6 and is read at 6")]
fn merges(w: &mut Steps) -> TestResult {
    let store = new_store();
    let k = friend();
    let complete = PostingList::default()
        .with_pack(ultragraphdb_core_codec::encode(&[1, 2], 256).context("the uids pack")?);
    write_entries(
        &store,
        vec![Entry::put(k.clone(), 4, BIT_COMPLETE_POSTING, marshal(&complete)).with_discard()],
    )?;
    write_delta(&store, &k, 5, &[set_uid(3)])?;
    write_delta(&store, &k, 6, &[del_uid(3), set_uid(4)])?;
    let lists = lists_over(store, ListsConfig::default());
    w.uids = uids(&lists.read(&k, 6)?, 6, 0)?;
    Ok(())
}

#[then("the result is the complete list with both deltas applied in commit order")]
fn merged(w: &mut Steps) {
    // The delta at 6 deletes what the delta at 5 set: applied out of order, 3 would remain.
    assert_eq!(w.uids, vec![1, 2, 4]);
}

#[when("a list with deltas at 3 and 5 was rolled up into a complete version at 6 and is read at 4")]
fn below_rollup(w: &mut Steps) -> TestResult {
    let store = new_store();
    let k = friend();
    write_delta(&store, &k, 3, &[set_uid(3)])?;
    write_delta(&store, &k, 5, &[set_uid(5)])?;
    let list = read_back(&store, &k)?;
    let (_, version) = roll_up(&store, &list, 6, MAX_LIST_SIZE)?;
    assert_eq!(version, 6);
    let lists = lists_over(store, ListsConfig::default());
    w.uids = uids(&lists.read(&k, 4)?, 4, 0)?;
    Ok(())
}

#[then("the result is the list as of the delta at 3")]
fn as_of_three(w: &mut Steps) {
    assert_eq!(w.uids, vec![3]);
}

#[when("a list holds a delta at 4, a delete-all at 5 and an empty rollup at 6, and is read at 4")]
fn below_empty_rollup(w: &mut Steps) -> TestResult {
    let store = new_store();
    let k = friend();
    write_delta(&store, &k, 4, &[set_uid(4)])?;
    write_delta(&store, &k, 5, &[delete_all()])?;
    write_entries(
        &store,
        vec![Entry::put(k.clone(), 6, BIT_EMPTY_POSTING, Vec::new()).with_discard()],
    )?;
    let lists = lists_over(store, ListsConfig::default());
    w.uids = uids(&lists.read(&k, 4)?, 4, 0)?;
    Ok(())
}

#[then("the result holds the delta's postings")]
fn holds_delta(w: &mut Steps) {
    // Dgraph serves the empty rollup to this read (mvcc.go:708-709).
    assert_eq!(w.uids, vec![4]);
}

// Posting lists: delete-all.

#[when("a list holds postings, a delete-all is committed at 8, and the list is read at 8 and at 7")]
fn delete_all_at_eight(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let k = friend();
    commit(&lists, &k, &[set_uid(1), set_uid(2)])?;
    assert_eq!(commit_edges_at(&lists, &k, &[delete_all()], 8)?, 8);
    w.uids = uids(&lists.read(&k, 8)?, 8, 0)?;
    w.second = uids(&lists.read(&k, 7)?, 7, 0)?;
    Ok(())
}

#[then("the read at 8 is empty and the read at 7 returns the postings")]
fn empty_at_eight(w: &mut Steps) {
    assert!(w.uids.is_empty());
    assert_eq!(w.second, vec![1, 2]);
}

// Posting lists: rollups.

#[when("a list with a complete version and several deltas is rolled up")]
fn rollup_preserves(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let k = friend();
    commit(&lists, &k, &(1..=5).map(set_uid).collect::<Vec<_>>())?;
    assert!(lists.rollup_key(&k)?);
    commit(&lists, &k, &[del_uid(2)])?;
    commit(&lists, &k, &[set_uid(7)])?;
    let last = commit_edges(&lists, &k, &[set_uid(9), del_uid(1)])?;
    // The watermark passes the reads below with a commit of another list.
    commit_at(&lists, &key("preserved", 1), &[set_uid(1)], last + 6)?;
    let before: Vec<Vec<u64>> = (last..last + 6)
        .map(|t| uids(&lists.read_uncached(&k, t)?, t, 0))
        .collect::<TestResult<_>>()?;
    assert!(lists.rollup_key(&k)?);
    let version = lists.store().contents().versions(&k)[0].version();
    assert_eq!(version, last + 1);
    let after: Vec<Vec<u64>> = (last..last + 6)
        .map(|t| uids(&lists.read_uncached(&k, t)?, t, 0))
        .collect::<TestResult<_>>()?;
    w.reads = vec![before, after];
    w.versions = vec![version, last];
    Ok(())
}

#[then(
    "reads at every timestamp at or above the rollup's version return the same results as before it"
)]
fn rollup_preserved(w: &mut Steps) {
    let (before, after) = (&w.reads[0], &w.reads[1]);
    assert_eq!(before, after);
    assert_eq!(before[0], vec![3, 4, 5, 7, 9]);
}

/// The list of TestRollupTimestamp: deltas at 2, 4 and 6 and a delete-all at 10.
fn rollup_timestamp_list() -> TestResult<List> {
    let store = new_store();
    let k = friend();
    write_delta(&store, &k, 2, &[set_uid(2)])?;
    write_delta(&store, &k, 4, &[set_uid(3)])?;
    write_delta(&store, &k, 6, &[set_uid(4)])?;
    write_delta(&store, &k, 10, &[delete_all()])?;
    read_back(&store, &k)
}

#[when(
    "a list holds deltas committed at 2, 4 and 6 and a delete-all committed at 10, and is rolled up at the maximum timestamp"
)]
fn rollup_max(w: &mut Steps) -> TestResult {
    let list = rollup_timestamp_list()?;
    let (version, entries) = list
        .rollup_entries(u64::MAX, MAX_LIST_SIZE)?
        .context("a rollup")?;
    assert!(entries.iter().all(|e| e.version() == version));
    w.versions = vec![version];
    w.list = Some(list);
    Ok(())
}

#[then("the complete version is written at 10, the delete-all's commit timestamp")]
fn at_ten(w: &mut Steps) {
    assert_eq!(w.versions, vec![10]);
}

#[when("the same list is rolled up at timestamp 12")]
fn rollup_twelve(w: &mut Steps) -> TestResult {
    let list = rollup_timestamp_list()?;
    let (version, _) = list
        .rollup_entries(12, MAX_LIST_SIZE)?
        .context("a rollup")?;
    w.versions = vec![version];
    Ok(())
}

#[then("the complete version is written at 11")]
fn at_eleven(w: &mut Steps) {
    assert_eq!(w.versions, vec![11]);
}

#[when("a reference posting carrying a facet is rolled up")]
fn facet_rollup(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let k = friend();
    commit(&lists, &k, &[with_facet(set_uid(5), "since")])?;
    assert!(lists.rollup_key(&k)?);
    // The rollup is written one above the commit it folds, past the watermark: a read of everything reaches it.
    let ts = u64::MAX;
    w.list = Some(lists.read_uncached(&k, ts)?);
    w.versions = vec![ts];
    Ok(())
}

#[then("the posting and its facet are read back after the rollup")]
fn facet_back(w: &mut Steps) -> TestResult {
    let list = w.list.as_ref().context("the list")?;
    assert!(
        list.mutation().is_empty(),
        "read from the rolled-up version"
    );
    let p = list
        .find_posting(w.versions[0], 5)?
        .context("the posting")?;
    assert_eq!(p.facets()[0].key(), "since");
    Ok(())
}

/// A split list in `store` of uids 1..=1500, split at 1,000 bytes, with uids 1501..=1600 committed above it.
fn split_with_deltas(seed: u64) -> TestResult<(TestLists, Vec<u8>)> {
    let store = SimStore::new(Seed::new(seed), FaultConfig::none(), SimClock::new());
    let lists = lists_over(store, ListsConfig::new(PersistMode::Buffer, 1000));
    let k = friend();
    commit(&lists, &k, &(1..=1500).map(set_uid).collect::<Vec<_>>())?;
    assert!(lists.rollup_key(&k)?);
    commit(&lists, &k, &(1501..=1600).map(set_uid).collect::<Vec<_>>())?;
    Ok((lists, k))
}

#[when(
    "the process crashes while the rollup of a split list is written, and the store is reopened"
)]
fn crash_split_rollup(w: &mut Steps) -> TestResult {
    for seed in 0..16 {
        let (lists, k) = split_with_deltas(seed)?;
        let old = lists.store().contents().versions(&k)[0].version();
        assert!(read_back(lists.store(), &k)?.part_splits().len() > 1);
        lists.store().inject(Fault::CrashDuringNextBatch);
        assert!(matches!(
            lists
                .rollup_key(&k)
                .failure("the rollup through a crash")?
                .0,
            PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
        ));
        lists.store().recover();
        let store = lists.store().clone();
        let reopened = lists_over(store, lists.config());
        let ts = reopened.applied_ts();
        let list = reopened.read_uncached(&k, ts)?;
        w.flags
            .push(uids(&list, ts, 0)? == (1..=1600).collect::<Vec<_>>());
        let main = reopened.store().contents().versions(&k)[0].clone();
        // The old list's complete version, or the new one above it.
        assert!(main.version() == old || main.version() > old);
        let _new_outcome: bool = w.outcomes.insert(u64::from(main.version() > old));
    }
    Ok(())
}

#[then(
    "the list reads as it did before the rollup, with its old parts, or as after it, with its new parts"
)]
fn whole_after_crash(w: &mut Steps) {
    assert!(w.flags.iter().all(|&f| f), "a list read in part");
    assert_eq!(w.outcomes.len(), 2, "both outcomes occur across the seeds");
}

// Posting lists: splits.

#[when("a list grows past 512 KiB over several blocks and is rolled up")]
fn grows(w: &mut Steps) -> TestResult {
    let store = new_store();
    let k = friend();
    let mut list = read_back(&store, &k)?;
    (1..=60_000).for_each(|u| add(&mut list, 1, &with_facet(set_uid(u), "f")));
    list.commit_mutation(2);
    let (list, version) = roll_up(&store, &list, u64::MAX, MAX_LIST_SIZE)?;
    w.versions = vec![version];
    w.list = Some(list);
    w.store = Some(store);
    Ok(())
}

#[then("its complete version records its parts and each part is stored under a split key")]
fn parts_recorded(w: &mut Steps) -> TestResult {
    let list = w.list.as_ref().context("the list")?;
    let store = w.store.as_ref().context("the store")?;
    let splits = list.part_splits();
    assert!(splits.len() > 1);
    assert_eq!(splits[0], 1);
    let contents = store.contents();
    let main = &contents.versions(&friend())[0];
    assert_eq!(main.meta(), BIT_COMPLETE_POSTING);
    for start in splits {
        let part = split_key_of(&friend(), start)?;
        assert_eq!(store.contents().versions(&part)[0].version(), w.versions[0]);
    }
    Ok(())
}

#[when("a split list is iterated from a uid in its first part")]
fn iterate_split(w: &mut Steps) -> TestResult {
    let (lists, k) = split_with_deltas(1)?;
    assert!(lists.rollup_key(&k)?);
    let ts = lists.applied_ts();
    let list = lists.read(&k, ts)?;
    assert!(list.part_splits().len() > 2);
    assert!(list.part_splits()[1] > 10);
    w.uids = uids(&list, ts, 10)?;
    Ok(())
}

#[then("the result is every uid above it, across all parts, in order")]
fn every_uid_above(w: &mut Steps) {
    assert_eq!(w.uids, (11..=1600).collect::<Vec<_>>());
}

#[when("every uid of a split list is deleted and the list is rolled up")]
fn delete_split(w: &mut Steps) -> TestResult {
    let (lists, k) = split_with_deltas(2)?;
    assert!(lists.rollup_key(&k)?);
    let splits = read_back(lists.store(), &k)?.part_splits();
    assert_eq!(
        split_keys_held(lists.store(), &k)?,
        splits.len(),
        "each part is held under a split key"
    );
    commit(&lists, &k, &(1..=1600).map(del_uid).collect::<Vec<_>>())?;
    assert!(lists.rollup_key(&k)?);
    w.uids = splits;
    w.store = Some(lists.store().clone());
    w.key = k;
    Ok(())
}

#[then("the list is stored as empty and no split key of the list remains")]
fn stored_empty(w: &mut Steps) -> TestResult {
    let store = w.store.as_ref().context("the store")?;
    assert_eq!(
        store.contents().versions(&w.key)[0].meta(),
        BIT_EMPTY_POSTING
    );
    assert_eq!(
        split_keys_held(store, &w.key)?,
        0,
        "a split key of the list is still read"
    );
    assert!(w.uids.len() > 1);
    Ok(())
}

/// How many split keys of `key`'s list a read at the maximum timestamp finds: the keys under the list's key with the
/// split flag set on its keyspace byte.
fn split_keys_held(store: &SimStore, key: &[u8]) -> TestResult<usize> {
    let prefix = [
        &[key[0] | ultragraphdb_core_keys::SPLIT_FLAG][..],
        &key[1..],
    ]
    .concat();
    Ok(store
        .snapshot(u64::MAX)?
        .scan(&ultragraphdb_traits_store::Scan::prefix(prefix))
        .count())
}

#[when("a split key is read directly as a posting list")]
fn read_split_key(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let part = split_key_of(&friend(), 1)?;
    w.error = lists.read(&part, lists.applied_ts()).err();
    Ok(())
}

#[then("the read fails with `cannot read posting list using multi-part list key`")]
fn invalid_key(w: &mut Steps) -> TestResult {
    let error = w.error.as_ref().context("an error")?;
    assert_eq!(
        error.to_string(),
        "cannot read posting list using multi-part list key"
    );
    Ok(())
}

// The local commit.

#[when("`name` of `0x1` holds no value and a transaction sets it to `A` and then deletes `A`")]
fn set_then_delete(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let ts = commit_edges_at(&lists, &name(), &[set_value("A"), del_value("A")], 2)?;
    w.texts = value_of(&lists.read(&name(), ts)?, ts)?
        .into_iter()
        .collect();
    Ok(())
}

#[then("after the commit, `name` of `0x1` holds no value")]
fn holds_no_value(w: &mut Steps) -> TestResult {
    assert!(w.texts.is_empty(), "{:?}", w.texts);
    Ok(())
}

#[when(
    "two transactions of one round execute from the same watermark and the first sets `name` of `0x1` to `A`"
)]
fn two_in_round(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let before = commit_edges_at(&lists, &name(), &[set_value("before")], 2)?;
    let watermark = lists.applied_ts();
    assert_eq!(watermark, before);
    let mut first = lists.request(watermark);
    first.get(&name())?.add_posting(set_value("A"), false);
    let mut second = lists.request(watermark);
    w.texts = value_of(second.get(&name())?, watermark)?
        .into_iter()
        .collect();
    let (first, second) = (first.into_writes(), second.into_writes());
    lists
        .commit_round(&[(4, &first), (6, &second)])
        .context("the round is written")?;
    w.second = vec![lists.applied_ts()];
    Ok(())
}

#[then("the second reads `name` of `0x1` as it was at the watermark")]
fn second_reads_watermark(w: &mut Steps) {
    assert_eq!(w.texts, vec![b"before".to_vec()]);
    assert_eq!(w.second, vec![6]);
}

#[when(
    "one transaction has set `name` on `0x1` and not committed, and a second transaction reads it"
)]
fn uncommitted(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let ts = commit_edges_at(&lists, &name(), &[set_value("before")], 2)?;
    let mut first = lists.request(ts);
    first
        .get(&name())?
        .add_posting(set_value("after").clone(), false);
    let mut second = lists.request(ts);
    w.texts = value_of(second.get(&name())?, ts)?.into_iter().collect();
    Ok(())
}

#[then("the second transaction sees the value before the first transaction's write")]
fn sees_before(w: &mut Steps) {
    assert_eq!(w.texts, vec![b"before".to_vec()]);
}

#[when(
    "`friend` of `0x1` holds `0x2` in the store, and a transaction adds `0x3` without reading the list and then reads it"
)]
fn blind(w: &mut Steps) -> TestResult {
    let store = new_store();
    write_delta(&store, &friend(), 2, &[set_uid(2)])?;
    let lists = lists_over(store, ListsConfig::default());
    let read_ts = lists.applied_ts();
    let mut request = lists.request(read_ts);
    request
        .get_from_delta(&friend())
        .add_posting(set_uid(3).clone(), false);
    w.uids = uids(request.get(&friend())?, read_ts, 0)?;
    Ok(())
}

#[then("the read holds `0x2` and `0x3`")]
fn holds_both(w: &mut Steps) {
    assert_eq!(w.uids, vec![2, 3]);
}

/// Commits `keys` as one round at `versions`, one transaction per version, the keys shared out in order (one
/// transaction writing every key when there is one version); while the round is written, a reader at `read_ts` asks the
/// gate and reads the store underneath three times.
fn commit_while_reading(
    keys: &[Vec<u8>],
    versions: &[u64],
    read_ts: u64,
) -> TestResult<(Arc<ProbeLists>, Vec<bool>, Vec<usize>)> {
    let (lists, probe) = probe_lists();
    let inside = Arc::clone(&lists);
    let inner = probe.inner().clone();
    let seen = Arc::new(Mutex::new((Vec::new(), Vec::new())));
    let record = Arc::clone(&seen);
    let watched: Vec<Vec<u8>> = keys.to_vec();
    probe.on_next_apply(Box::new(move || -> TestResult {
        let mut record = record.lock().unwrap_or_else(PoisonError::into_inner);
        for _ in 0..3 {
            record.0.push(inside.try_gate(read_ts)?);
            let snapshot = inner.snapshot(read_ts)?;
            let mut held = 0;
            for k in &watched {
                if snapshot.get(k)?.is_some() {
                    held += 1;
                }
            }
            record.1.push(held);
        }
        Ok(())
    }));
    let per_transaction = keys.len().div_ceil(versions.len());
    let writes: Vec<Writes> = keys
        .chunks(per_transaction)
        .map(|chunk| {
            let mut request = lists.request(lists.applied_ts());
            for k in chunk {
                request.get(k)?.add_posting(set_uid(1), false);
            }
            Ok(request.into_writes())
        })
        .collect::<TestResult<_>>()?;
    let round: Vec<(u64, &Writes)> = versions.iter().copied().zip(&writes).collect();
    lists.commit_round(&round)?;
    let (gates, during) = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    Ok((lists, gates, during))
}

/// How many of `keys` a read at `read_ts` after the commit finds written.
fn written_after(lists: &ProbeLists, keys: &[Vec<u8>], read_ts: u64) -> TestResult<Vec<usize>> {
    let mut written = Vec::new();
    for _ in 0..3 {
        let mut found = 0;
        for k in keys {
            if !uids(&lists.read(k, read_ts)?, read_ts, 0)?.is_empty() {
                found += 1;
            }
        }
        written.push(found);
    }
    Ok(written)
}

#[when(
    "a transaction writing three lists commits at 12 while another reader reads at 12 repeatedly"
)]
fn three_lists(w: &mut Steps) -> TestResult {
    let keys: Vec<Vec<u8>> = (1..=3).map(|u| key("whole", u)).collect();
    let (lists, gates, during) = commit_while_reading(&keys, &[12], 12)?;
    w.flags = gates;
    w.uids = during.into_iter().map(|n| n as u64).collect();
    w.second = written_after(&lists, &keys, 12)?
        .into_iter()
        .map(|n| n as u64)
        .collect();
    Ok(())
}

#[then("every read sees the changes to all three lists or to none of them")]
fn all_or_none(w: &mut Steps) -> TestResult {
    // While the round is written the gate holds a read at 12 back, and the store underneath shows none of it.
    assert!(w.flags.iter().all(|&open| !open));
    assert!(w.uids.iter().all(|&n| n == 0 || n == 3), "{:?}", w.uids);
    assert_eq!(w.second, vec![3, 3, 3]);
    Ok(())
}

#[when(
    "one round holds a commit at 12 and a commit at 14 while another reader reads at 14 repeatedly"
)]
fn two_commits(w: &mut Steps) -> TestResult {
    let keys: Vec<Vec<u8>> = (1..=2).map(|u| key("together", u)).collect();
    let (lists, gates, during) = commit_while_reading(&keys, &[12, 14], 14)?;
    w.flags = gates;
    w.uids = during.into_iter().map(|n| n as u64).collect();
    w.second = written_after(&lists, &keys, 14)?
        .into_iter()
        .map(|n| n as u64)
        .collect();
    w.versions = keys
        .iter()
        .map(|k| lists.store().inner().contents().versions(k)[0].version())
        .collect();
    Ok(())
}

#[then("every read sees both commits or neither")]
fn both_or_neither(w: &mut Steps) -> TestResult {
    assert!(w.flags.iter().all(|&open| !open));
    assert!(w.uids.iter().all(|&n| n == 0 || n == 2), "{:?}", w.uids);
    assert_eq!(w.second, vec![2, 2, 2]);
    assert_eq!(w.versions, vec![12, 14]);
    Ok(())
}

#[when("a transaction aborts after building writes")]
fn failed_request(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let mut request = lists.request(lists.applied_ts());
    request
        .get(&friend())?
        .add_posting(set_uid(1).clone(), false);
    request
        .get_from_delta(&key("friend", 2))
        .add_posting(set_uid(2).clone(), false);
    // The transaction aborts: its writes are dropped, never handed to a round.
    drop(request.into_writes());
    w.store = Some(lists.store().clone());
    w.flags = vec![lists.oldest_open_read().is_none()];
    Ok(())
}

#[then("the store holds none of its writes")]
fn holds_none(w: &mut Steps) -> TestResult {
    let contents = w.store.as_ref().context("the store")?.contents();
    assert!(contents.keys().is_empty());
    assert_eq!(contents.applied_ts(), 0);
    assert_eq!(w.flags, vec![true], "its read is closed");
    Ok(())
}

#[when("a transaction aborts and another transaction begins")]
fn failed_holds_nothing(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let watermark = commit_edges_at(&lists, &key("friend", 9), &[set_uid(9)], 2)?;
    let mut request = lists.request(lists.applied_ts());
    request
        .get(&friend())?
        .add_posting(set_uid(1).clone(), false);
    drop(request);
    let begin = lists.applied_ts();
    w.versions = vec![watermark, begin];
    w.flags = vec![lists.try_gate(begin)?];
    let mut second = lists.request(begin);
    w.uids = uids(second.get(&friend())?, begin, 0)?;
    Ok(())
}

#[then("the second transaction begins at the watermark without waiting for the aborted one")]
fn no_wait(w: &mut Steps) {
    assert_eq!(w.versions[0], w.versions[1]);
    assert_eq!(w.flags, vec![true]);
    assert!(w.uids.is_empty());
}

#[when("a commit at 10 is being applied and a read at 10 starts")]
fn read_waits(w: &mut Steps) -> TestResult {
    let k = vec![key("gate", 1)];
    let (lists, gates, during) = commit_while_reading(&k, &[10], 10)?;
    w.flags = gates;
    w.uids = during.into_iter().map(|n| n as u64).collect();
    w.flags.push(lists.try_gate(10)?);
    w.second = uids(&lists.read(&k[0], 10)?, 10, 0)?;
    Ok(())
}

#[then("the read returns after the commit is applied and sees it")]
fn waited(w: &mut Steps) {
    assert_eq!(w.flags, vec![false, false, false, true]);
    assert_eq!(w.uids, vec![0, 0, 0]);
    assert_eq!(w.second, vec![1]);
}

#[when("a read at 6 is open and a list with deltas at 6 and 8 is rolled up")]
fn open_read_rollup(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let k = friend();
    commit_at(&lists, &k, &[set_uid(6)], 6)?;
    let mut request = lists.request(6);
    commit_at(&lists, &k, &[set_uid(8)], 8)?;
    assert!(lists.rollup_key(&k)?);
    let rolled = lists.store().contents().versions(&k)[0].version();
    w.uids = uids(request.get(&k)?, 6, 0)?;
    lists.set_discard_ts(100)?;
    w.versions = vec![rolled, lists.store().discard_ts()];
    Ok(())
}

#[then(
    "the rollup is written at 9, the read at 6 still returns its result, and the discard timestamp stays at or below 6"
)]
fn open_read_intact(w: &mut Steps) {
    assert_eq!(w.versions[0], 9);
    assert_eq!(w.uids, vec![6]);
    assert!(w.versions[1] <= 6);
}

// Rollups and commit versions.

#[when("a list with deltas at 10 and 12 is rolled up")]
fn rollup_between(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let k = key("between", 1);
    commit_at(&lists, &k, &[set_uid(10)], 10)?;
    commit_at(&lists, &k, &[set_uid(12)], 12)?;
    assert!(lists.rollup_key(&k)?);
    let versions = lists.store().contents().versions(&k).to_vec();
    w.versions = vec![versions[0].version(), lists.next_version()];
    w.flags = vec![versions[0].meta() & BIT_COMPLETE_POSTING != 0];
    Ok(())
}

#[then("its complete version is written at 13, and the next commit's version is even")]
fn written_between(w: &mut Steps) {
    assert_eq!(w.flags, vec![true]);
    assert_eq!(w.versions[0], 13);
    assert_eq!(w.versions[1] % 2, 0);
    assert!(w.versions[1] > 13);
}

// The posting cache.

#[when("a list is cached, a commit at 20 changes it, and it is read at 21")]
fn cached_then_commit(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let k = friend();
    let ts = commit_edges(&lists, &k, &[set_uid(1)])?;
    let _k_list: List = lists.read(&k, ts)?;
    commit_at(&lists, &k, &[set_uid(2)], 20)?;
    // A read at 21 waits until the watermark reaches it: a commit of another list at 22 takes it there.
    commit_at(&lists, &key("other", 1), &[set_uid(3)], 22)?;
    w.uids = uids(&lists.cached(&k, 21)?.context("a cache hit")?, 21, 0)?;
    Ok(())
}

#[then("the read reflects the commit at 20")]
fn reflects_twenty(w: &mut Steps) {
    assert_eq!(w.uids, vec![1, 2]);
}

/// One seeded run of commits and reads at increasing timestamps over three keys, with a cache of `capacity`
/// entries: what each read returned.
fn interleaved(seed: u64, capacity: usize) -> TestResult<Vec<Vec<u64>>> {
    let keys: Vec<Vec<u8>> = (1..=3).map(|u| key("concurrent", u)).collect();
    let mut lists: Option<TestLists> = None;
    let mut reads = Vec::new();
    // The first failed step of the run, after which the run's steps do nothing: the run's workload can fail only
    // with a violation, so the failure is carried out beside it.
    let mut failure: Option<TestError> = None;
    let mut simulation = Simulation::new(Seed::new(seed), FaultConfig::none());
    let _outcome: RunOutcome = simulation.run(300, 3, |ctx, event| {
        let lists = lists.get_or_insert_with(|| {
            let cache = SimListCache::new(capacity, ctx.seed().stream(Stream::Cache));
            TestLists::new(ctx.store().clone(), cache, ListsConfig::default())
        });
        let Event::Step(actor) = event else {
            return Ok(());
        };
        if failure.is_none() {
            failure = interleaved_step(ctx, lists, &keys, actor, &mut reads).err();
        }
        Ok(())
    })?;
    failure.map_or(Ok(reads), Err)
}

/// One step of [`interleaved`]'s run by `actor`: the first actor commits to one of `keys` and now and then rolls
/// it up, the others read one of them, adding what they read to `reads`.
fn interleaved_step(
    ctx: &mut SimContext,
    lists: &TestLists,
    keys: &[Vec<u8>],
    actor: usize,
    reads: &mut Vec<Vec<u64>>,
) -> TestResult {
    let k = keys[ctx.rng().index(keys.len())].clone();
    if actor == 0 {
        let uid = ctx.rng().between(1, 20);
        let edge = if ctx.rng().chance(300) {
            del_uid(uid)
        } else {
            set_uid(uid)
        };
        let _version: u64 = commit_edges(lists, &k, &[edge])?;
        if ctx.rng().chance(100) {
            let _rolled: bool = lists.rollup_key(&k)?;
        }
    } else {
        let ts = lists.applied_ts();
        reads.push(uids(&lists.read(&k, ts)?, ts, 0)?);
    }
    Ok(())
}

#[when(
    "commits and reads at increasing timestamps run concurrently against the same keys with the cache on and with it off"
)]
fn cache_on_and_off(w: &mut Steps) -> TestResult {
    for seed in 1..=8 {
        w.reads.push(interleaved(seed, 1024)?);
        w.reads.push(interleaved(seed, 0)?);
    }
    Ok(())
}

#[then("every read returns the same result in both runs")]
fn same_in_both(w: &mut Steps) {
    for pair in w.reads.chunks(2) {
        assert!(!pair[0].is_empty());
        assert_eq!(pair[0], pair[1]);
    }
}

#[when("a commit writes a list that is cached")]
fn commit_cached(w: &mut Steps) -> TestResult {
    let (lists, probe) = probe_lists();
    let k = friend();
    let mut first_batch = lists.batch();
    let mut first_request = first_batch.request(lists.applied_ts());
    first_request
        .get(&k)?
        .add_posting(set_uid(1).clone(), false);
    first_batch.push(first_request);
    let first = lists.commit(first_batch)?[0];
    let _k_list: List = lists.read(&k, first)?;
    assert!(lists.cache().get(&k).is_some());

    let commit_ts = lists.next_version();
    let inside = Arc::clone(&lists);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    probe.on_next_apply(Box::new(move || -> TestResult {
        // Before the first_batch is in the store, a read at its timestamp is held back.
        record
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(inside.try_gate(commit_ts)?);
        Ok(())
    }));
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request.get(&k)?.add_posting(set_uid(2).clone(), false);
    batch.push(request);
    let _versions: Vec<u64> = lists.commit(batch)?;
    w.flags = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    w.uids = uids(
        &lists.cached(&k, commit_ts)?.context("a hit")?,
        commit_ts,
        0,
    )?;
    Ok(())
}

#[then("no read that sees the batch in the store returns the cached list from before the batch")]
fn no_stale_read(w: &mut Steps) {
    assert_eq!(w.flags, vec![false]);
    assert_eq!(w.uids, vec![1, 2]);
}

#[when("a reader misses, a commit then writes the key, and the reader then inserts its list")]
fn stale_fill(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let k = friend();
    let first = commit_edges(&lists, &k, &[set_uid(1)])?;
    lists.cache().remove(&k);
    let miss = lists.miss(&k, first)?;
    let second = commit_edges(&lists, &k, &[set_uid(2)])?;
    lists.cache().remove(&k);
    w.flags = vec![lists.fill(miss), lists.cache().get(&k).is_some()];
    w.uids = uids(&lists.read(&k, second)?, second, 0)?;
    Ok(())
}

#[then("the insert is refused and a read at the commit's timestamp reflects the commit")]
fn fill_refused(w: &mut Steps) {
    assert_eq!(w.flags, vec![false, false]);
    assert_eq!(w.uids, vec![1, 2]);
}

/// Drops the friend predicate of the root namespace through `lists`: its data and split prefixes.
fn drop_friend(lists: &TestLists) -> TestResult {
    let prefix = |split| {
        ultragraphdb_core_keys::key_prefix(
            ultragraphdb_core_keys::Keyspace::Forward,
            split,
            0,
            Some(ultragraphdb_core_keys::DEFAULT_GRAPH),
            Some(crate::utils::helpers::predicate("friend")),
        )
    };
    let (data, split) = (prefix(false), prefix(true));
    lists.drop_prefixes(lists.applied_ts(), &[data, split], Vec::new())?;
    Ok(())
}

#[when("a cached predicate is dropped")]
fn drop_cached(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let k = friend();
    let ts = commit_edges(&lists, &k, &[set_uid(1)])?;
    let _k_list: List = lists.read(&k, ts)?;
    assert!(lists.cache().get(&k).is_some());
    drop_friend(&lists)?;
    let after = lists.applied_ts();
    w.uids = uids(&lists.read(&k, after)?, after, 0)?;
    w.second = uids(&lists.read(&k, ts)?, ts, 0)?;
    Ok(())
}

#[then("a read of that predicate finds nothing")]
fn finds_nothing(w: &mut Steps) {
    assert!(w.uids.is_empty());
    assert!(
        w.second.is_empty(),
        "a read below the drop finds nothing of it either"
    );
}

#[when(
    "a reader misses a list, the list's predicate is then dropped, and the reader inserts its list"
)]
fn fill_across_drop(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let k = friend();
    let ts = commit_edges(&lists, &k, &[set_uid(1)])?;
    lists.cache().remove(&k);
    let miss = lists.miss(&k, ts)?;
    drop_friend(&lists)?;
    w.flags = vec![lists.fill(miss)];
    let after = lists.applied_ts();
    w.uids = uids(&lists.read(&k, after)?, after, 0)?;
    Ok(())
}

#[then("the insert is refused, and a read of the predicate finds nothing")]
fn refused_after_drop(w: &mut Steps) {
    assert_eq!(w.flags, vec![false]);
    assert!(w.uids.is_empty());
}

// Storage simulation.

#[when("the posting-list operations of a workload run on the simulated store")]
fn workload(w: &mut Steps) -> TestResult {
    for seed in 1..=10 {
        let (_, reads, _) = PostingWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 2)
            .map_err(|v| v.to_string())
            .context("the workload breaks no invariant")?;
        w.uids.push(reads as u64);
        let (_, faulty_reads, _) =
            PostingWorkload::run(Seed::new(seed), PostingWorkload::faults(), 300, 2)
                .map_err(|v| v.to_string())
                .context("the workload breaks no invariant")?;
        w.second.push(faulty_reads as u64);
    }
    Ok(())
}

#[then("they pass the same checks as on fjall")]
fn same_checks(w: &mut Steps) {
    // The checks are the workload's model rules, written against the Store trait; ultragraphdb_store_kv runs the
    // same workload over fjall once it lands.
    assert!(
        w.uids.iter().all(|&r| r > 0),
        "a fault-free run compared no read"
    );
    assert!(
        w.second.iter().sum::<u64>() > 0,
        "the faulty runs compared no read"
    );
}

#[when("a stored posting list's bytes are corrupted and the list is read")]
fn corrupted(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let k = friend();
    commit(&lists, &k, &[set_uid(1), set_uid(2), set_uid(3)])?;
    assert!(lists.rollup_key(&k)?);
    let version = lists.store().contents().versions(&k)[0].version();
    // The bytes past the store's version prefix are garbled; the store returns them, and the posting decoder
    // refuses them: here the flip breaks the pack's structure.
    lists
        .store()
        .corrupt(&k, version, Corruption::FlipValueByte);
    lists.cache().remove(&k);
    // The rollup is written one above the commit it folds, past the watermark: a read of everything reaches it.
    let ts = u64::MAX;
    let uncached = lists.read_uncached(&k, ts).err();
    let cached = lists.read(&k, ts).err();
    w.key = k;
    w.flags = vec![cached.is_some() && cached == uncached];
    w.error = uncached;
    Ok(())
}

#[then("the read fails with an error naming the key, and no value is returned")]
fn refused_naming_key(w: &mut Steps) -> TestResult {
    let error = w.error.as_ref().context("the read is refused")?;
    let hex: String = w.key.iter().map(|b| format!("{b:02x}")).collect();
    assert!(matches!(
        &error.0,
        PostingErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted { key, .. })) if *key == w.key
    ));
    assert!(error.to_string().contains(&hex), "{error}");
    assert_eq!(
        w.flags,
        vec![true],
        "the read through the cache is refused too"
    );
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
