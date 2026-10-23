/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/mvcc_test.go (TestRollupTimestamp, TestIncrRollupGetsCancelledQuickly) and
 * the rollup triggers of posting/mvcc.go (ReadPostingList, checkForRollup, CommitToDisk, ProcessDelta,
 * addKeyToBatch, Process). Changed for ultragraphdb: translated to Rust; the rollup's timestamp comes from the
 * local counter where Go's test fixes it at 100, and a paused rollup returns at once where Go's waits on Zero's
 * timestamp until its closer is signalled; the port's discards and the worker on the stepped executor are tested
 * beside them.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::Arc;

use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING, HIGH_PRIORITY_DELTAS, ListOptions, PostingErrorEnum,
    ROLLUP_BATCH, ROLLUP_QUEUE,
};
use ultragraphdb_store_sim::{Seed, SteppedExecutor, Stream};
use ultragraphdb_traits_store::{Entry, ListCache, PersistMode, Store, WriteBatch};

use crate::utils::helpers::{
    TestLists, commit_edges, delete_all, key, new_lists, next_commit_at, probe_lists, set_uid,
    uids, write_delta,
};

#[test]
fn test_rollup_timestamp() {
    let lists = new_lists();
    let k = key("rollup", 1);
    // Three delta commits.
    write_delta(lists.store(), &k, 2, &[set_uid(2)]);
    write_delta(lists.store(), &k, 4, &[set_uid(3)]);
    write_delta(lists.store(), &k, 6, &[set_uid(4)]);
    let l = lists.read_uncached(&k, u64::MAX - 1).unwrap();
    assert_eq!(l.uids(&ListOptions::at(7)).unwrap().uids.len(), 3);

    write_delta(lists.store(), &k, 10, &[delete_all()]);
    let nl = lists.read_uncached(&k, u64::MAX - 1).unwrap();
    assert!(nl.uids(&ListOptions::at(11)).unwrap().uids.is_empty());

    // The highest version survives a rollup at the maximum timestamp, the delete-all being the newest update.
    let (version, entries) = nl.rollup_entries(u64::MAX, usize::MAX).unwrap().unwrap();
    assert_eq!(version, 10);
    assert_eq!(entries[0].version(), 10);
    // At a finite timestamp the rollup is written one above.
    let (version, _) = nl.rollup_entries(12, usize::MAX).unwrap().unwrap();
    assert_eq!(version, 11);
}

#[test]
fn test_incr_rollup_gets_cancelled_quickly() {
    let lists = new_lists();
    let k = key("rollup", 1);
    commit_edges(&lists, &k, &[set_uid(1)], None);
    lists.pause_rollups();
    assert_eq!(
        lists.rollup_key(&k).unwrap_err().0,
        PostingErrorEnum::HighPriorityOp
    );
    lists.resume_rollups();
    assert!(lists.rollup_key(&k).unwrap());
}

#[test]
fn test_a_rollup_writes_one_batch_at_the_main_keys_version_and_clears_the_entry() {
    let lists = new_lists();
    let k = key("rollup-batch", 1);
    let last = commit_edges(&lists, &k, &[set_uid(1)], None);
    lists.read(&k, last).unwrap();
    assert!(lists.cache().get(&k).is_some());
    let applied = lists.applied_ts();
    assert!(lists.rollup_key(&k).unwrap());
    assert!(
        lists.cache().get(&k).is_none(),
        "the rollup removed the entry"
    );
    assert_eq!(
        lists.applied_ts(),
        applied,
        "a rollup leaves the applied timestamp"
    );
    let contents = lists.store().contents();
    let stored = &contents.versions(&k)[0];
    assert_eq!(
        (stored.version(), stored.meta(), stored.discard()),
        (last + 1, BIT_COMPLETE_POSTING, true)
    );
}

#[test]
fn test_a_list_without_deltas_is_not_rolled_up() {
    let lists = new_lists();
    let k = key("rollup-nothing", 1);
    assert!(!lists.rollup_key(&k).unwrap());
    commit_edges(&lists, &k, &[set_uid(1)], None);
    assert!(lists.rollup_key(&k).unwrap());
    assert!(
        !lists.rollup_key(&k).unwrap(),
        "the rolled-up list has no deltas"
    );
}

#[test]
fn test_a_rollup_whose_epoch_moved_after_its_read_is_discarded() {
    let (lists, probe) = probe_lists();
    let k = key("rollup-epoch", 1);
    commit_edges_probe(&lists, &k);
    let inside = Arc::clone(&lists);
    // The rollup's read opens a snapshot; a drop's epoch change follows it before the rollup writes.
    probe.on_next_snapshot(Box::new(move || inside.advance_epoch()));
    let before = probe.inner().contents();
    assert!(!lists.rollup_key(&k).unwrap());
    assert_eq!(probe.inner().contents(), before);
}

#[test]
fn test_a_rollup_whose_key_gained_a_deletion_is_discarded() {
    let (lists, probe) = probe_lists();
    let k = key("rollup-deleted", 1);
    commit_edges_probe(&lists, &k);
    let store = probe.inner().clone();
    let deleted = k.clone();
    probe.on_next_snapshot(Box::new(move || {
        let mut batch = WriteBatch::new();
        batch.push(Entry::deletion(deleted.clone(), 1000));
        store.apply(batch, PersistMode::Buffer).unwrap();
    }));
    assert!(!lists.rollup_key(&k).unwrap());
    let versions = probe.inner().contents().versions(&k).to_vec();
    assert!(
        versions.iter().all(|v| v.meta() != BIT_COMPLETE_POSTING),
        "nothing written"
    );
}

fn commit_edges_probe(lists: &crate::utils::helpers::ProbeLists, k: &[u8]) {
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request
        .get(k)
        .unwrap()
        .add_mutation(&set_uid(1), None)
        .unwrap();
    batch.push(request);
    lists.commit(batch).unwrap();
}

#[test]
fn test_a_rollup_leaves_an_open_read_intact() {
    let lists = new_lists();
    let k = key("open-read", 1);
    next_commit_at(&lists, 5);
    commit_edges(&lists, &k, &[set_uid(5)], None);
    let mut request = lists.request(5);
    next_commit_at(&lists, 6);
    commit_edges(&lists, &k, &[set_uid(6)], None);
    assert!(lists.rollup_key(&k).unwrap());
    let rolled = lists.store().contents().versions(&k)[0].version();
    assert!(rolled > 6);
    assert_eq!(uids(request.get(&k).unwrap(), 5, 0), vec![5]);
    lists.set_discard_ts(100).unwrap();
    assert!(lists.store().discard_ts() <= 5);
}

#[test]
fn test_an_emptied_list_rolls_up_empty() {
    let lists = new_lists();
    let k = key("rollup-empty", 1);
    commit_edges(&lists, &k, &[set_uid(1)], None);
    commit_edges(&lists, &k, &[delete_all()], None);
    assert!(lists.rollup_key(&k).unwrap());
    assert_eq!(
        lists.store().contents().versions(&k)[0].meta(),
        BIT_EMPTY_POSTING
    );
}

fn waiting_after(lists: &TestLists, keys: usize, priority: usize) -> usize {
    (0..keys).for_each(|i| lists.add_rollup_key(key("queue", i as u64), priority));
    lists.rollups_waiting()
}

#[test]
fn test_rollup_triggers() {
    let lists = new_lists();
    // A commit queues each key it wrote at both priorities, as CommitToDisk and ProcessDelta do.
    let k = key("trigger", 1);
    commit_edges(&lists, &k, &[set_uid(1)], None);
    assert_eq!(lists.rollups_waiting(), 2);
    // A read with deltas queues the key at low priority.
    lists.read_uncached(&k, lists.applied_ts()).unwrap();
    assert_eq!(lists.rollups_waiting(), 3);
    // A cache hit on a list of more than 500 deltas queues it at high priority, as checkForRollup does.
    let big = key("trigger", 2);
    for uid in 0..=HIGH_PRIORITY_DELTAS as u64 {
        commit_edges(&lists, &big, &[set_uid(uid + 1)], None);
    }
    let before = lists.rollups_waiting();
    lists.read(&big, lists.applied_ts()).unwrap();
    let hit = lists.cached(&big, lists.applied_ts()).unwrap();
    assert!(hit.is_some());
    assert!(lists.rollups_waiting() > before);
}

#[test]
fn test_the_queues_drop_batches_beyond_their_capacity() {
    let lists = new_lists();
    let all = waiting_after(&lists, ROLLUP_BATCH * (ROLLUP_QUEUE + 3), 1);
    assert_eq!(
        all,
        ROLLUP_BATCH * ROLLUP_QUEUE,
        "batches beyond the queue are dropped"
    );
}

#[test]
fn test_the_worker_rolls_up_what_waits_once_per_ten_seconds() {
    let lists = new_lists();
    let keys: Vec<_> = (1..=3).map(|i| key("worker", i)).collect();
    keys.iter().for_each(|k| {
        commit_edges(&lists, k, &[set_uid(1)], None);
    });
    // The high-priority keys gathered are taken without a full batch, as Dgraph's forced tick takes them.
    assert_eq!(lists.run_rollups(100).unwrap(), 3);
    keys.iter().for_each(|k| {
        commit_edges(&lists, k, &[set_uid(2)], None);
    });
    assert_eq!(
        lists.run_rollups(105).unwrap(),
        0,
        "rolled up five seconds ago"
    );
    keys.iter().for_each(|k| lists.add_rollup_key(k.clone(), 0));
    assert_eq!(lists.run_rollups(110).unwrap(), 3);
}

#[test]
fn test_the_worker_runs_on_the_executor() {
    let lists = Arc::new(new_lists());
    let k = key("executor", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)], None);
    let executor = SteppedExecutor::new(Seed::new(3).stream(Stream::Executor));
    lists.spawn_rollups(&executor, 50);
    assert_eq!(executor.pending(), 1);
    executor.run_until_idle();
    assert_eq!(
        lists.store().contents().versions(&k)[0].meta(),
        BIT_COMPLETE_POSTING
    );
    assert_eq!(uids(&lists.read(&k, ts).unwrap(), ts, 0), vec![1]);
}

#[test]
fn test_a_discarded_rollup_is_reported() {
    let capture = ultragraphdb_store_sim::LogCapture::new();
    tracing::subscriber::with_default(capture.subscriber(), || {
        let (lists, probe) = probe_lists();
        let k = key("rollup-reported", 1);
        commit_edges_probe(&lists, &k);
        let inside = Arc::clone(&lists);
        probe.on_next_snapshot(Box::new(move || inside.advance_epoch()));
        assert!(!lists.rollup_key(&k).unwrap());
    });
    let events = capture.events();
    let discarded = events
        .iter()
        .find(|e| {
            e.message()
                .is_some_and(|m| m.starts_with("rollup discarded"))
        })
        .expect("the discard is reported");
    assert_eq!(discarded.level(), tracing::Level::INFO);
    assert!(discarded.field("key").is_some() && discarded.field("rollup_ts").is_some());
}
