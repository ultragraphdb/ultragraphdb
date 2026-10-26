/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use cucumber::{World, then, when};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_store_posting::split_key_of;
use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING, List, ListsConfig, MAX_LIST_SIZE, PostingError,
    PostingErrorEnum, marshal,
};
use ultragraphdb_store_sim::{
    Corruption, Event, Fault, FaultConfig, Seed, SimClock, SimListCache, SimStore, Simulation,
    Stream,
};
use ultragraphdb_traits_store::{Entry, ListCache, PersistMode, Store, StoreSnapshot};

// The unit tests' helpers and the posting workload, shared by path.
mod utils {
    #[allow(dead_code)]
    #[path = "helpers.rs"]
    pub mod helpers;
    #[path = "posting_workload.rs"]
    pub mod posting_workload;
}

use crate::utils::helpers::{
    ProbeLists, TestLists, add, commit_edges, del_uid, del_value, delete_all, key, lists_over,
    new_lists, new_store, next_commit_at, probe_lists, read_back, roll_up, set_uid, set_value,
    uids, with_facet, write_delta, write_entries,
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
fn value_of(list: &List, read_ts: u64) -> Option<Vec<u8>> {
    let mut value = None;
    list.iterate(read_ts, 0, |p| {
        value = Some(p.value.clone());
        Ok::<_, PostingError>(std::ops::ControlFlow::Break(()))
    })
    .expect("the list iterates");
    value
}

// Posting lists: reads.

#[when("a list holds a delta committed at 6 and is read at 5")]
fn later_delta(w: &mut Steps) {
    let store = new_store();
    write_delta(&store, &friend(), 6, &[set_uid(9)]);
    let lists = lists_over(store, ListsConfig::default());
    w.uids = uids(&lists.read(&friend(), 5).unwrap(), 5, 0);
}

#[then("the result excludes the delta's postings")]
fn excludes_delta(w: &mut Steps) {
    assert!(w.uids.is_empty(), "{:?}", w.uids);
}

#[when("a list holds a complete version at 4 and deltas at 5 and 6 and is read at 6")]
fn merges(w: &mut Steps) {
    let store = new_store();
    let k = friend();
    let complete = PostingList {
        pack: ultragraphdb_core_codec::encode(&[1, 2], 256),
        ..PostingList::default()
    };
    write_entries(
        &store,
        vec![Entry::put(k.clone(), 4, BIT_COMPLETE_POSTING, marshal(&complete)).with_discard()],
    );
    write_delta(&store, &k, 5, &[set_uid(3)]);
    write_delta(&store, &k, 6, &[del_uid(3), set_uid(4)]);
    let lists = lists_over(store, ListsConfig::default());
    w.uids = uids(&lists.read(&k, 6).unwrap(), 6, 0);
}

#[then("the result is the complete list with both deltas applied in commit order")]
fn merged(w: &mut Steps) {
    // The delta at 6 deletes what the delta at 5 set: applied out of order, 3 would remain.
    assert_eq!(w.uids, vec![1, 2, 4]);
}

#[when("a list with deltas at 3 and 5 was rolled up into a complete version at 6 and is read at 4")]
fn below_rollup(w: &mut Steps) {
    let store = new_store();
    let k = friend();
    write_delta(&store, &k, 3, &[set_uid(3)]);
    write_delta(&store, &k, 5, &[set_uid(5)]);
    let list = read_back(&store, &k);
    let (_, version) = roll_up(&store, &list, 6, MAX_LIST_SIZE);
    assert_eq!(version, 6);
    let lists = lists_over(store, ListsConfig::default());
    w.uids = uids(&lists.read(&k, 4).unwrap(), 4, 0);
}

#[then("the result is the list as of the delta at 3")]
fn as_of_three(w: &mut Steps) {
    assert_eq!(w.uids, vec![3]);
}

#[when("a list holds a delta at 4, a delete-all at 5 and an empty rollup at 6, and is read at 4")]
fn below_empty_rollup(w: &mut Steps) {
    let store = new_store();
    let k = friend();
    write_delta(&store, &k, 4, &[set_uid(4)]);
    write_delta(&store, &k, 5, &[delete_all()]);
    write_entries(
        &store,
        vec![Entry::put(k.clone(), 6, BIT_EMPTY_POSTING, Vec::new()).with_discard()],
    );
    let lists = lists_over(store, ListsConfig::default());
    w.uids = uids(&lists.read(&k, 4).unwrap(), 4, 0);
}

#[then("the result holds the delta's postings")]
fn holds_delta(w: &mut Steps) {
    // Dgraph serves the empty rollup to this read (mvcc.go:708-709).
    assert_eq!(w.uids, vec![4]);
}

// Posting lists: delete-all.

#[when("a list holds postings, a delete-all is committed at 8, and the list is read at 8 and at 7")]
fn delete_all_at_eight(w: &mut Steps) {
    let lists = new_lists();
    let k = friend();
    commit_edges(&lists, &k, &[set_uid(1), set_uid(2)]);
    next_commit_at(&lists, 8);
    assert_eq!(commit_edges(&lists, &k, &[delete_all()]), 8);
    w.uids = uids(&lists.read(&k, 8).unwrap(), 8, 0);
    w.second = uids(&lists.read(&k, 7).unwrap(), 7, 0);
}

#[then("the read at 8 is empty and the read at 7 returns the postings")]
fn empty_at_eight(w: &mut Steps) {
    assert!(w.uids.is_empty());
    assert_eq!(w.second, vec![1, 2]);
}

// Posting lists: rollups.

#[when("a list with a complete version and several deltas is rolled up")]
fn rollup_preserves(w: &mut Steps) {
    let lists = new_lists();
    let k = friend();
    commit_edges(&lists, &k, &(1..=5).map(set_uid).collect::<Vec<_>>());
    assert!(lists.rollup_key(&k).unwrap());
    commit_edges(&lists, &k, &[del_uid(2)]);
    commit_edges(&lists, &k, &[set_uid(7)]);
    let last = commit_edges(&lists, &k, &[set_uid(9), del_uid(1)]);
    let before: Vec<Vec<u64>> = (last..last + 6)
        .map(|t| uids(&lists.read_uncached(&k, t).unwrap(), t, 0))
        .collect();
    assert!(lists.rollup_key(&k).unwrap());
    let version = lists.store().contents().versions(&k)[0].version();
    assert_eq!(version, last + 1);
    let after: Vec<Vec<u64>> = (last..last + 6)
        .map(|t| uids(&lists.read_uncached(&k, t).unwrap(), t, 0))
        .collect();
    w.reads = vec![before, after];
    w.versions = vec![version, last];
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
fn rollup_timestamp_list() -> List {
    let store = new_store();
    let k = friend();
    write_delta(&store, &k, 2, &[set_uid(2)]);
    write_delta(&store, &k, 4, &[set_uid(3)]);
    write_delta(&store, &k, 6, &[set_uid(4)]);
    write_delta(&store, &k, 10, &[delete_all()]);
    read_back(&store, &k)
}

#[when(
    "a list holds deltas committed at 2, 4 and 6 and a delete-all committed at 10, and is rolled up at the maximum timestamp"
)]
fn rollup_max(w: &mut Steps) {
    let list = rollup_timestamp_list();
    let (version, entries) = list
        .rollup_entries(u64::MAX, MAX_LIST_SIZE)
        .unwrap()
        .unwrap();
    assert!(entries.iter().all(|e| e.version() == version));
    w.versions = vec![version];
    w.list = Some(list);
}

#[then("the complete version is written at 10, the delete-all's commit timestamp")]
fn at_ten(w: &mut Steps) {
    assert_eq!(w.versions, vec![10]);
}

#[when("the same list is rolled up at timestamp 12")]
fn rollup_twelve(w: &mut Steps) {
    let list = rollup_timestamp_list();
    let (version, _) = list.rollup_entries(12, MAX_LIST_SIZE).unwrap().unwrap();
    w.versions = vec![version];
}

#[then("the complete version is written at 11")]
fn at_eleven(w: &mut Steps) {
    assert_eq!(w.versions, vec![11]);
}

#[when("a reference posting carrying a facet is rolled up")]
fn facet_rollup(w: &mut Steps) {
    let lists = new_lists();
    let k = friend();
    commit_edges(&lists, &k, &[with_facet(set_uid(5), "since")]);
    assert!(lists.rollup_key(&k).unwrap());
    let ts = lists.latest_ts();
    w.list = Some(lists.read_uncached(&k, ts).unwrap());
    w.versions = vec![ts];
}

#[then("the posting and its facet are read back after the rollup")]
fn facet_back(w: &mut Steps) {
    let list = w.list.as_ref().unwrap();
    assert!(
        list.mutation().is_empty(),
        "read from the rolled-up version"
    );
    let p = list
        .find_posting(w.versions[0], 5)
        .unwrap()
        .expect("the posting");
    assert_eq!(p.facets[0].key, "since");
}

/// A split list in `store` of uids 1..=1500, split at 1,000 bytes, with uids 1501..=1600 committed above it.
fn split_with_deltas(seed: u64) -> (TestLists, Vec<u8>) {
    let store = SimStore::new(Seed::new(seed), FaultConfig::none(), SimClock::new());
    let lists = lists_over(store, ListsConfig::new(PersistMode::Buffer, 1000));
    let k = friend();
    commit_edges(&lists, &k, &(1..=1500).map(set_uid).collect::<Vec<_>>());
    assert!(lists.rollup_key(&k).unwrap());
    commit_edges(&lists, &k, &(1501..=1600).map(set_uid).collect::<Vec<_>>());
    (lists, k)
}

#[when(
    "the process crashes while the rollup of a split list is written, and the store is reopened"
)]
fn crash_split_rollup(w: &mut Steps) {
    for seed in 0..16 {
        let (lists, k) = split_with_deltas(seed);
        let old = lists.store().contents().versions(&k)[0].version();
        assert!(read_back(lists.store(), &k).part_splits().len() > 1);
        lists.store().inject(Fault::CrashDuringNextBatch);
        assert!(lists.rollup_key(&k).is_err());
        lists.store().recover();
        let store = lists.store().clone();
        let reopened = lists_over(store, lists.config());
        let ts = reopened.latest_ts();
        let list = reopened.read_uncached(&k, ts).unwrap();
        w.flags
            .push(uids(&list, ts, 0) == (1..=1600).collect::<Vec<_>>());
        let main = reopened.store().contents().versions(&k)[0].clone();
        // The old list's complete version, or the new one above it.
        assert!(main.version() == old || main.version() > old);
        w.outcomes.insert(u64::from(main.version() > old));
    }
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
fn grows(w: &mut Steps) {
    let store = new_store();
    let k = friend();
    let mut list = read_back(&store, &k);
    (1..=60_000).for_each(|u| add(&mut list, 1, &with_facet(set_uid(u), "f")));
    list.commit_mutation(2);
    let (list, version) = roll_up(&store, &list, u64::MAX, MAX_LIST_SIZE);
    w.versions = vec![version];
    w.list = Some(list);
    w.store = Some(store);
}

#[then("its complete version records its parts and each part is stored under a split key")]
fn parts_recorded(w: &mut Steps) {
    let list = w.list.as_ref().unwrap();
    let store = w.store.as_ref().unwrap();
    let splits = list.part_splits();
    assert!(splits.len() > 1);
    assert_eq!(splits[0], 1);
    let contents = store.contents();
    let main = &contents.versions(&friend())[0];
    assert_eq!(main.meta(), BIT_COMPLETE_POSTING);
    for start in splits {
        let part = split_key_of(&friend(), start).unwrap();
        assert_eq!(store.contents().versions(&part)[0].version(), w.versions[0]);
    }
}

#[when("a split list is iterated from a uid in its first part")]
fn iterate_split(w: &mut Steps) {
    let (lists, k) = split_with_deltas(1);
    assert!(lists.rollup_key(&k).unwrap());
    let ts = lists.latest_ts();
    let list = lists.read(&k, ts).unwrap();
    assert!(list.part_splits().len() > 2);
    assert!(list.part_splits()[1] > 10);
    w.uids = uids(&list, ts, 10);
}

#[then("the result is every uid above it, across all parts, in order")]
fn every_uid_above(w: &mut Steps) {
    assert_eq!(w.uids, (11..=1600).collect::<Vec<_>>());
}

#[when("every uid of a split list is deleted and the list is rolled up")]
fn delete_split(w: &mut Steps) {
    let (lists, k) = split_with_deltas(2);
    assert!(lists.rollup_key(&k).unwrap());
    let splits = read_back(lists.store(), &k).part_splits();
    commit_edges(&lists, &k, &(1..=1600).map(del_uid).collect::<Vec<_>>());
    assert!(lists.rollup_key(&k).unwrap());
    w.uids = splits;
    w.store = Some(lists.store().clone());
    w.key = k;
}

#[then("the list is stored as empty and no split key of the list remains")]
fn stored_empty(w: &mut Steps) {
    let store = w.store.as_ref().unwrap();
    assert_eq!(
        store.contents().versions(&w.key)[0].meta(),
        BIT_EMPTY_POSTING
    );
    let snapshot = store.snapshot(u64::MAX).unwrap();
    let parts = snapshot
        .scan(&ultragraphdb_traits_store::Scan::prefix(
            [&[0x04][..], &w.key[1..]].concat(),
        ))
        .count();
    assert_eq!(parts, 0, "a split key of the list is still read");
    assert!(w.uids.len() > 1);
}

#[when("a split key is read directly as a posting list")]
fn read_split_key(w: &mut Steps) {
    let lists = new_lists();
    let part = split_key_of(&friend(), 1).unwrap();
    w.error = lists.read(&part, lists.latest_ts()).err();
}

#[then("the read fails with `cannot read posting list using multi-part list key`")]
fn invalid_key(w: &mut Steps) {
    let error = w.error.as_ref().expect("an error");
    assert_eq!(
        error.to_string(),
        "cannot read posting list using multi-part list key"
    );
}

// The local commit.

#[when("`name` of `0x1` holds no value and a request sets it to `A` and then deletes `A`")]
fn set_then_delete(w: &mut Steps) {
    let lists = new_lists();
    let ts = commit_edges(&lists, &name(), &[set_value("A"), del_value("A")]);
    w.texts = value_of(&lists.read(&name(), ts).unwrap(), ts)
        .into_iter()
        .collect();
}

#[then("after the commit, `name` of `0x1` holds no value")]
fn holds_no_value(w: &mut Steps) {
    assert!(w.texts.is_empty(), "{:?}", w.texts);
}

#[when("two requests are applied in one batch and the first sets `name` of `0x1` to `A`")]
fn two_in_batch(w: &mut Steps) {
    let lists = new_lists();
    let mut batch = lists.batch();
    let mut first = batch.request(lists.latest_ts());
    first
        .get(&name())
        .unwrap()
        .add_posting(set_value("A").clone(), false);
    batch.push(first);
    let read_ts = lists.latest_ts();
    let mut second = batch.request(read_ts);
    w.texts = value_of(second.get(&name()).unwrap(), read_ts)
        .into_iter()
        .collect();
}

#[then("the second reads `A` for `name` of `0x1`")]
fn second_reads_a(w: &mut Steps) {
    assert_eq!(w.texts, vec![b"A".to_vec()]);
}

#[when("one request has set `name` on `0x1` and not committed, and a second request reads it")]
fn uncommitted(w: &mut Steps) {
    let lists = new_lists();
    let ts = commit_edges(&lists, &name(), &[set_value("before")]);
    let mut first = lists.request(ts);
    first
        .get(&name())
        .unwrap()
        .add_posting(set_value("after").clone(), false);
    let mut second = lists.request(ts);
    w.texts = value_of(second.get(&name()).unwrap(), ts)
        .into_iter()
        .collect();
}

#[then("the second request sees the value before the first request's write")]
fn sees_before(w: &mut Steps) {
    assert_eq!(w.texts, vec![b"before".to_vec()]);
}

#[when(
    "`friend` of `0x1` holds `0x2` in the store, and a request adds `0x3` without reading the list and then reads it"
)]
fn blind(w: &mut Steps) {
    let store = new_store();
    write_delta(&store, &friend(), 2, &[set_uid(2)]);
    let lists = lists_over(store, ListsConfig::default());
    let read_ts = lists.latest_ts();
    let mut request = lists.request(read_ts);
    request
        .get_from_delta(&friend())
        .add_posting(set_uid(3).clone(), false);
    w.uids = uids(request.get(&friend()).unwrap(), read_ts, 0);
}

#[then("the read holds `0x2` and `0x3`")]
fn holds_both(w: &mut Steps) {
    assert_eq!(w.uids, vec![2, 3]);
}

/// Commits `keys`, one request each, in one batch whose first commit is at `first`; while the batch is written,
/// a reader at `read_ts` asks the gate and reads the store underneath three times.
fn commit_while_reading(
    keys: &[Vec<u8>],
    per_request: bool,
    first: u64,
    read_ts: u64,
) -> (Arc<ProbeLists>, Vec<bool>, Vec<usize>) {
    let (lists, probe) = probe_lists();
    next_commit_at(&*lists, first);
    let inside = Arc::clone(&lists);
    let inner = probe.inner().clone();
    let seen = Arc::new(Mutex::new((Vec::new(), Vec::new())));
    let record = Arc::clone(&seen);
    let watched: Vec<Vec<u8>> = keys.to_vec();
    probe.on_next_apply(Box::new(move || {
        let mut record = record.lock().unwrap();
        for _ in 0..3 {
            record.0.push(inside.try_gate(read_ts).unwrap());
            let snapshot = inner.snapshot(read_ts).unwrap();
            record.1.push(
                watched
                    .iter()
                    .filter(|k| snapshot.get(k).unwrap().is_some())
                    .count(),
            );
        }
    }));
    let mut batch = lists.batch();
    if per_request {
        for k in keys {
            let mut request = batch.request(lists.latest_ts());
            request
                .get(k)
                .unwrap()
                .add_posting(set_uid(1).clone(), false);
            batch.push(request);
        }
    } else {
        let mut request = batch.request(lists.latest_ts());
        for k in keys {
            request
                .get(k)
                .unwrap()
                .add_posting(set_uid(1).clone(), false);
        }
        batch.push(request);
    }
    let timestamps = lists.commit(batch).unwrap();
    assert_eq!(timestamps[0], first);
    let (gates, during) = seen.lock().unwrap().clone();
    (lists, gates, during)
}

/// How many of `keys` a read at `read_ts` after the commit finds written.
fn written_after(lists: &ProbeLists, keys: &[Vec<u8>], read_ts: u64) -> Vec<usize> {
    (0..3)
        .map(|_| {
            keys.iter()
                .filter(|k| !uids(&lists.read(k, read_ts).unwrap(), read_ts, 0).is_empty())
                .count()
        })
        .collect()
}

#[when("a request writing three lists commits at 12 while another reader reads at 12 repeatedly")]
fn three_lists(w: &mut Steps) {
    let keys: Vec<Vec<u8>> = (1..=3).map(|u| key("whole", u)).collect();
    let (lists, gates, during) = commit_while_reading(&keys, false, 12, 12);
    w.flags = gates;
    w.uids = during.into_iter().map(|n| n as u64).collect();
    w.second = written_after(&lists, &keys, 12)
        .into_iter()
        .map(|n| n as u64)
        .collect();
}

#[then("every read sees the changes to all three lists or to none of them")]
fn all_or_none(w: &mut Steps) {
    // While the batch is written the gate holds a read at 12 back, and the store underneath shows none of it.
    assert!(w.flags.iter().all(|&open| !open));
    assert!(w.uids.iter().all(|&n| n == 0 || n == 3), "{:?}", w.uids);
    assert_eq!(w.second, vec![3, 3, 3]);
}

#[when(
    "one batch holds a commit at 12 and a commit at 13 while another reader reads at 13 repeatedly"
)]
fn two_commits(w: &mut Steps) {
    let keys: Vec<Vec<u8>> = (1..=2).map(|u| key("together", u)).collect();
    let (lists, gates, during) = commit_while_reading(&keys, true, 12, 13);
    w.flags = gates;
    w.uids = during.into_iter().map(|n| n as u64).collect();
    w.second = written_after(&lists, &keys, 13)
        .into_iter()
        .map(|n| n as u64)
        .collect();
}

#[then("every read sees both commits or neither")]
fn both_or_neither(w: &mut Steps) {
    assert!(w.flags.iter().all(|&open| !open));
    assert!(w.uids.iter().all(|&n| n == 0 || n == 2), "{:?}", w.uids);
    assert_eq!(w.second, vec![2, 2, 2]);
}

#[when("a request fails after building writes and before committing")]
fn failed_request(w: &mut Steps) {
    let lists = new_lists();
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request
        .get(&friend())
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    request
        .get_from_delta(&key("friend", 2))
        .add_posting(set_uid(2).clone(), false);
    batch.push(request);
    // The request fails in its commit check, before any timestamp is assigned.
    let results = lists
        .commit_with(batch, |_| Err("conversion failed"))
        .unwrap();
    assert_eq!(results, vec![Err("conversion failed")]);
    w.store = Some(lists.store().clone());
}

#[then("the store holds none of its writes")]
fn holds_none(w: &mut Steps) {
    let contents = w.store.as_ref().unwrap().contents();
    assert!(contents.keys().is_empty());
    assert_eq!(contents.applied_ts(), 0);
}

#[when("a request fails before its batch is written and a read at the next timestamp starts")]
fn failed_holds_nothing(w: &mut Steps) {
    let lists = new_lists();
    let mut request = lists.request(lists.latest_ts());
    request
        .get(&friend())
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    drop(request);
    let next = lists.peek_ts();
    w.flags = vec![lists.try_gate(next).unwrap()];
    lists.gate(next).unwrap();
    w.uids = uids(&lists.read(&friend(), next).unwrap(), next, 0);
}

#[then("the read returns without waiting for the failed request")]
fn no_wait(w: &mut Steps) {
    assert_eq!(w.flags, vec![true]);
    assert!(w.uids.is_empty());
}

#[when("a commit at 10 is being written and a read at 11 starts")]
fn read_waits(w: &mut Steps) {
    let k = vec![key("gate", 1)];
    let (lists, gates, during) = commit_while_reading(&k, false, 10, 11);
    w.flags = gates;
    w.uids = during.into_iter().map(|n| n as u64).collect();
    w.flags.push(lists.try_gate(11).unwrap());
    w.second = uids(&lists.read(&k[0], 11).unwrap(), 11, 0);
}

#[then("the read returns after the commit is applied and sees it")]
fn waited(w: &mut Steps) {
    assert_eq!(w.flags, vec![false, false, false, true]);
    assert_eq!(w.uids, vec![0, 0, 0]);
    assert_eq!(w.second, vec![1]);
}

#[when("a read at 5 is open and a list with deltas at 5 and 6 is rolled up")]
fn open_read_rollup(w: &mut Steps) {
    let lists = new_lists();
    let k = friend();
    next_commit_at(&lists, 5);
    commit_edges(&lists, &k, &[set_uid(5)]);
    let mut request = lists.request(5);
    next_commit_at(&lists, 6);
    commit_edges(&lists, &k, &[set_uid(6)]);
    assert!(lists.rollup_key(&k).unwrap());
    let rolled = lists.store().contents().versions(&k)[0].version();
    w.uids = uids(request.get(&k).unwrap(), 5, 0);
    lists.set_discard_ts(100).unwrap();
    w.versions = vec![rolled, lists.store().discard_ts()];
}

#[then(
    "the rollup is written above 6, the read at 5 still returns its result, and the discard timestamp stays at or below 5"
)]
fn open_read_intact(w: &mut Steps) {
    assert!(w.versions[0] > 6);
    assert_eq!(w.uids, vec![5]);
    assert!(w.versions[1] <= 5);
}

// The posting cache.

#[when("a list is cached, a commit at 20 changes it, and it is read at 21")]
fn cached_then_commit(w: &mut Steps) {
    let lists = new_lists();
    let k = friend();
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.read(&k, ts).unwrap();
    next_commit_at(&lists, 20);
    commit_edges(&lists, &k, &[set_uid(2)]);
    w.uids = uids(&lists.cached(&k, 21).unwrap().expect("a cache hit"), 21, 0);
}

#[then("the read reflects the commit at 20")]
fn reflects_twenty(w: &mut Steps) {
    assert_eq!(w.uids, vec![1, 2]);
}

/// One seeded run of commits and reads at increasing timestamps over three keys, with a cache of `capacity`
/// entries: what each read returned.
fn interleaved(seed: u64, capacity: usize) -> Vec<Vec<u64>> {
    let keys: Vec<Vec<u8>> = (1..=3).map(|u| key("concurrent", u)).collect();
    let mut lists: Option<TestLists> = None;
    let mut reads = Vec::new();
    let mut simulation = Simulation::new(Seed::new(seed), FaultConfig::none());
    simulation
        .run(300, 3, |ctx, event| {
            let lists = lists.get_or_insert_with(|| {
                let cache = SimListCache::new(capacity, ctx.seed().stream(Stream::Cache));
                TestLists::new(ctx.store().clone(), cache, ListsConfig::default())
            });
            let Event::Step(actor) = event else {
                return Ok(());
            };
            let k = keys[ctx.rng().index(keys.len())].clone();
            if actor == 0 {
                let uid = ctx.rng().between(1, 20);
                let edge = if ctx.rng().chance(300) {
                    del_uid(uid)
                } else {
                    set_uid(uid)
                };
                commit_edges(lists, &k, &[edge]);
                if ctx.rng().chance(100) {
                    lists.rollup_key(&k).unwrap();
                }
            } else {
                let ts = lists.latest_ts();
                reads.push(uids(&lists.read(&k, ts).unwrap(), ts, 0));
            }
            Ok(())
        })
        .unwrap();
    reads
}

#[when(
    "commits and reads at increasing timestamps run concurrently against the same keys with the cache on and with it off"
)]
fn cache_on_and_off(w: &mut Steps) {
    for seed in 1..=8 {
        w.reads.push(interleaved(seed, 1024));
        w.reads.push(interleaved(seed, 0));
    }
}

#[then("every read returns the same result in both runs")]
fn same_in_both(w: &mut Steps) {
    for pair in w.reads.chunks(2) {
        assert!(!pair[0].is_empty());
        assert_eq!(pair[0], pair[1]);
    }
}

#[when("a commit writes a list that is cached")]
fn commit_cached(w: &mut Steps) {
    let (lists, probe) = probe_lists();
    let k = friend();
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request
        .get(&k)
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    batch.push(request);
    let first = lists.commit(batch).unwrap()[0];
    lists.read(&k, first).unwrap();
    assert!(lists.cache().get(&k).is_some());

    let commit_ts = lists.peek_ts();
    let inside = Arc::clone(&lists);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    probe.on_next_apply(Box::new(move || {
        // Before the batch is in the store, a read at its timestamp is held back.
        record
            .lock()
            .unwrap()
            .push(inside.try_gate(commit_ts).unwrap());
    }));
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request
        .get(&k)
        .unwrap()
        .add_posting(set_uid(2).clone(), false);
    batch.push(request);
    lists.commit(batch).unwrap();
    w.flags = seen.lock().unwrap().clone();
    w.uids = uids(
        &lists.cached(&k, commit_ts).unwrap().expect("a hit"),
        commit_ts,
        0,
    );
}

#[then("no read that sees the batch in the store returns the cached list from before the batch")]
fn no_stale_read(w: &mut Steps) {
    assert_eq!(w.flags, vec![false]);
    assert_eq!(w.uids, vec![1, 2]);
}

#[when("a reader misses, a commit then writes the key, and the reader then inserts its list")]
fn stale_fill(w: &mut Steps) {
    let lists = new_lists();
    let k = friend();
    let first = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.cache().remove(&k);
    let miss = lists.miss(&k, first).unwrap();
    let second = commit_edges(&lists, &k, &[set_uid(2)]);
    lists.cache().remove(&k);
    w.flags = vec![lists.fill(miss), lists.cache().get(&k).is_some()];
    w.uids = uids(&lists.read(&k, second).unwrap(), second, 0);
}

#[then("the insert is refused and a read at the commit's timestamp reflects the commit")]
fn fill_refused(w: &mut Steps) {
    assert_eq!(w.flags, vec![false, false]);
    assert_eq!(w.uids, vec![1, 2]);
}

/// Drops the friend predicate of the root namespace through `lists`: its data and split prefixes.
fn drop_friend(lists: &TestLists) {
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
    lists
        .drop_prefixes(lists.fresh_ts(), &[data, split], Vec::new())
        .unwrap();
}

#[when("a cached predicate is dropped")]
fn drop_cached(w: &mut Steps) {
    let lists = new_lists();
    let k = friend();
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.read(&k, ts).unwrap();
    assert!(lists.cache().get(&k).is_some());
    drop_friend(&lists);
    let after = lists.latest_ts();
    w.uids = uids(&lists.read(&k, after).unwrap(), after, 0);
    w.second = uids(&lists.read(&k, ts).unwrap(), ts, 0);
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
fn fill_across_drop(w: &mut Steps) {
    let lists = new_lists();
    let k = friend();
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.cache().remove(&k);
    let miss = lists.miss(&k, ts).unwrap();
    drop_friend(&lists);
    w.flags = vec![lists.fill(miss)];
    let after = lists.latest_ts();
    w.uids = uids(&lists.read(&k, after).unwrap(), after, 0);
}

#[then("the insert is refused, and a read of the predicate finds nothing")]
fn refused_after_drop(w: &mut Steps) {
    assert_eq!(w.flags, vec![false]);
    assert!(w.uids.is_empty());
}

// Storage simulation.

#[when("the posting-list operations of a workload run on the simulated store")]
fn workload(w: &mut Steps) {
    for seed in 1..=10 {
        let (_, reads, _) = PostingWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 2)
            .unwrap_or_else(|v| panic!("{v}"));
        w.uids.push(reads as u64);
        let (_, reads, _) =
            PostingWorkload::run(Seed::new(seed), PostingWorkload::faults(), 300, 2)
                .unwrap_or_else(|v| panic!("{v}"));
        w.second.push(reads as u64);
    }
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
fn corrupted(w: &mut Steps) {
    let lists = new_lists();
    let k = friend();
    commit_edges(&lists, &k, &[set_uid(1), set_uid(2), set_uid(3)]);
    assert!(lists.rollup_key(&k).unwrap());
    let version = lists.store().contents().versions(&k)[0].version();
    // The bytes past the store's version prefix are garbled; the store returns them, and the posting decoder
    // refuses them: here the flip breaks the pack's structure.
    lists
        .store()
        .corrupt(&k, version, Corruption::FlipValueByte);
    lists.cache().remove(&k);
    let ts = lists.latest_ts();
    let uncached = lists.read_uncached(&k, ts);
    let cached = lists.read(&k, ts);
    w.key = k;
    w.error = uncached.err();
    w.flags = vec![cached.is_err()];
}

#[then("the read fails with an error naming the key, and no value is returned")]
fn refused_naming_key(w: &mut Steps) {
    let error = w.error.as_ref().expect("the read is refused");
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
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
