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

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING, HIGH_PRIORITY_DELTAS, ListOptions, PostingError,
    PostingErrorEnum, ROLLUP_BATCH, ROLLUP_QUEUE,
};
use ultragraphdb_store_sim::{Fault, Seed, SteppedExecutor, Stream};
use ultragraphdb_traits_store::{Entry, ListCache, PersistMode, Store, WriteBatch};

use crate::utils::helpers::{
    TestLists, commit_edges, commit_edges_at, delete_all, key, new_lists, probe_lists, set_uid,
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
    let l = lists.read_uncached(&k, u64::MAX).unwrap();
    assert_eq!(l.uids(&ListOptions::at(7)).unwrap().uids().len(), 3);

    write_delta(lists.store(), &k, 10, &[delete_all()]);
    let nl = lists.read_uncached(&k, u64::MAX).unwrap();
    assert!(nl.uids(&ListOptions::at(11)).unwrap().uids().is_empty());

    // The highest version survives a rollup at the maximum timestamp, the delete-all being the newest update.
    let (version, entries) = nl.rollup_entries(u64::MAX, usize::MAX).unwrap().unwrap();
    assert_eq!(version, 10);
    assert_eq!(entries[0].version(), 10);
    // At a finite timestamp the rollup is written one above.
    let (version, _) = nl.rollup_entries(12, usize::MAX).unwrap().unwrap();
    assert_eq!(version, 11);
}

/// A rollup reads at the watermark, writes one above the newest commit it folds, an odd version, and takes no
/// version: the next commit's version is what it was, and a commit there lands above the rollup.
#[test]
fn test_a_rollup_reads_at_the_watermark_and_takes_no_version() {
    let lists = new_lists();
    let k = key("between", 1);
    commit_edges_at(&lists, &k, &[set_uid(10)], 10);
    commit_edges_at(&lists, &key("between", 2), &[set_uid(1)], 16);
    let next = lists.next_version();
    assert!(lists.rollup_key(&k).unwrap());
    let rolled = lists.store().contents().versions(&k)[0].version();
    assert_eq!(
        rolled, 11,
        "one above the newest commit it folds, not the watermark"
    );
    assert_eq!(lists.next_version(), next);
    assert_eq!(lists.oldest_open_read(), None);
    commit_edges_at(&lists, &k, &[set_uid(18)], next);
    assert_eq!(uids(&lists.read(&k, next).unwrap(), next, 0), vec![10, 18]);
}

#[test]
fn test_incr_rollup_gets_cancelled_quickly() {
    let lists = new_lists();
    let k = key("rollup", 1);
    commit_edges(&lists, &k, &[set_uid(1)]);
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
    let last = commit_edges(&lists, &k, &[set_uid(1)]);
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
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    assert!(lists.rollup_key(&k).unwrap());
    let rolled = lists.store().contents().versions(&k)[0].clone();
    assert_eq!(rolled.version(), ts + 1);
    // The rollup lies above the watermark, which still reads the delta: a second rollup writes the same list again.
    assert!(lists.rollup_key(&k).unwrap());
    assert_eq!(lists.store().contents().versions(&k)[0], rolled);
    // Once the watermark passes the rollup, the list has no deltas to roll up.
    commit_edges(&lists, &key("rollup-nothing", 2), &[set_uid(1)]);
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
    let mut request = batch.request(lists.applied_ts());
    request
        .get(k)
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    batch.push(request);
    lists.commit(batch).unwrap();
}

#[test]
fn test_a_rollup_leaves_an_open_read_intact() {
    let lists = new_lists();
    let k = key("open-read", 1);
    commit_edges_at(&lists, &k, &[set_uid(6)], 6);
    let mut request = lists.request(6);
    commit_edges_at(&lists, &k, &[set_uid(8)], 8);
    assert!(lists.rollup_key(&k).unwrap());
    let rolled = lists.store().contents().versions(&k)[0].version();
    assert_eq!(rolled, 9);
    assert_eq!(uids(request.get(&k).unwrap(), 6, 0), vec![6]);
    lists.set_discard_ts(100).unwrap();
    assert!(lists.store().discard_ts() <= 6);
}

#[test]
fn test_an_emptied_list_rolls_up_empty() {
    let lists = new_lists();
    let k = key("rollup-empty", 1);
    commit_edges(&lists, &k, &[set_uid(1)]);
    commit_edges(&lists, &k, &[delete_all()]);
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
    commit_edges(&lists, &k, &[set_uid(1)]);
    assert_eq!(lists.rollups_waiting(), 2);
    // A read with deltas queues the key at low priority.
    lists.read_uncached(&k, lists.applied_ts()).unwrap();
    assert_eq!(lists.rollups_waiting(), 3);
    // A cache hit on a list of more than 500 deltas queues it at high priority, as checkForRollup does.
    let big = key("trigger", 2);
    for uid in 0..=HIGH_PRIORITY_DELTAS as u64 {
        commit_edges(&lists, &big, &[set_uid(uid + 1)]);
    }
    let before = lists.rollups_waiting();
    lists.read(&big, lists.applied_ts()).unwrap();
    assert_eq!(lists.rollups_waiting(), before + 1, "the read was a hit");
    let hit = lists.cached(&big, lists.applied_ts()).unwrap();
    assert_eq!(
        hit.map(|l| l.mutation().len()),
        Some(HIGH_PRIORITY_DELTAS + 1)
    );
    assert_eq!(lists.rollups_waiting(), before + 2);
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
        commit_edges(&lists, k, &[set_uid(1)]);
    });
    // The high-priority keys gathered are taken without a full batch, as Dgraph's forced tick takes them.
    assert_eq!(lists.run_rollups(100).unwrap(), 3);
    keys.iter().for_each(|k| {
        commit_edges(&lists, k, &[set_uid(2)]);
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
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
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
    capture.capture(|| {
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

/// Writes `n` deltas of `k` straight to the store, so that no commit queues the key.
/// Writes `n` deltas of `k` straight to `store`, at 2, 4, and on, as Go's tests commit by hand; lists opened over
/// the store after read at the newest of them.
fn deltas(store: &ultragraphdb_store_sim::SimStore, k: &[u8], n: u64) {
    (1..=n).for_each(|i| write_delta(store, k, 2 * i, &[set_uid(i)]));
}

#[test]
fn test_a_read_of_more_than_500_deltas_queues_its_key_at_high_priority() {
    // HIGH_PRIORITY_DELTAS is Dgraph's 500: a read of 500 deltas queues its key at low priority, which waits for a
    // full batch, and one of 501 at high priority, which the worker takes at once.
    let store = crate::utils::helpers::new_store();
    let (low, high) = (key("priority", 1), key("priority", 2));
    deltas(&store, &low, HIGH_PRIORITY_DELTAS as u64);
    deltas(&store, &high, HIGH_PRIORITY_DELTAS as u64 + 1);
    let lists = crate::utils::helpers::lists_over(
        store,
        ultragraphdb_store_posting::ListsConfig::default(),
    );
    let read_ts = lists.applied_ts();
    lists.read_uncached(&low, read_ts).unwrap();
    assert_eq!(lists.rollups_waiting(), 1);
    assert_eq!(
        lists.run_rollups(100).unwrap(),
        0,
        "a low-priority key waits"
    );
    lists.read_uncached(&high, read_ts).unwrap();
    assert_eq!(lists.rollups_waiting(), 2);
    assert_eq!(
        lists.run_rollups(100).unwrap(),
        1,
        "the high-priority key is rolled up"
    );
    assert_eq!(lists.rollups_waiting(), 1);
    assert_eq!(
        lists.store().contents().versions(&high)[0].meta(),
        BIT_COMPLETE_POSTING
    );
}

#[test]
fn test_a_cache_hit_on_more_than_500_deltas_queues_its_key() {
    let store = crate::utils::helpers::new_store();
    let (at, above) = (key("hit-priority", 1), key("hit-priority", 2));
    deltas(&store, &at, HIGH_PRIORITY_DELTAS as u64);
    deltas(&store, &above, HIGH_PRIORITY_DELTAS as u64 + 1);
    let lists = crate::utils::helpers::lists_over(
        store,
        ultragraphdb_store_posting::ListsConfig::default(),
    );
    let read_ts = lists.applied_ts();
    lists.read(&at, read_ts).unwrap();
    lists.read(&above, read_ts).unwrap();
    let waiting = lists.rollups_waiting();
    assert!(lists.cached(&at, read_ts).unwrap().is_some());
    assert_eq!(
        lists.rollups_waiting(),
        waiting,
        "500 deltas do not queue on a hit"
    );
    assert!(lists.cached(&above, read_ts).unwrap().is_some());
    assert_eq!(lists.rollups_waiting(), waiting + 1);
}

#[test]
fn test_the_worker_stops_at_a_failed_write_and_skips_a_key_it_cannot_roll_up() {
    let capture = ultragraphdb_store_sim::LogCapture::new();
    capture.capture(|| {
        let lists = new_lists();
        let k = key("worker-skip", 1);
        commit_edges(&lists, &k, &[set_uid(1)]);
        let statement = b"\x17not a list".to_vec();
        lists.add_rollup_key(statement.clone(), 0);
        // The commit queued k at both priorities; the statement key is refused and skipped.
        assert_eq!(lists.run_rollups(100).unwrap(), 1);

        commit_edges(&lists, &k, &[set_uid(2)]);
        lists.store().inject(Fault::FailNextBatch);
        let error = lists.run_rollups(200).unwrap_err();
        assert!(matches!(
            error.0,
            PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
        ));
        assert_eq!(
            commit_edges_result(&lists, &k),
            Err(PostingError::Store(StoreError::Poisoned())),
            "a failed rollup stops every later write"
        );
    });
    let warned: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|e| e.message() == Some("error rolling up key"))
        .collect();
    assert_eq!(warned.len(), 1);
    assert_eq!(warned[0].level(), tracing::Level::WARN);
    // The key as Go's %x prints it.
    assert_eq!(warned[0].field("key"), Some("176e6f742061206c697374"));
}

#[test]
fn test_the_worker_stops_once_a_commit_failed() {
    let lists = new_lists();
    let k = key("worker-poisoned", 1);
    commit_edges(&lists, &k, &[set_uid(1)]);
    lists.store().inject(Fault::FailNextBatch);
    assert!(commit_edges_result(&lists, &k).is_err());
    assert_eq!(
        lists.run_rollups(100),
        Err(PostingError::Store(StoreError::Poisoned()))
    );
}

#[test]
fn test_rollups_resume_only_when_every_pause_has_ended() {
    let lists = new_lists();
    let k = key("pauses", 1);
    commit_edges(&lists, &k, &[set_uid(1)]);
    lists.pause_rollups();
    lists.pause_rollups();
    lists.resume_rollups();
    assert_eq!(
        lists.rollup_key(&k).unwrap_err().0,
        PostingErrorEnum::HighPriorityOp
    );
    lists.resume_rollups();
    lists.resume_rollups();
    assert!(
        lists.rollup_key(&k).unwrap(),
        "an extra resume does not pause"
    );
}

/// A pause that comes while a rollup waits for its timestamp's commits cancels the rollup.
#[test]
fn test_a_pause_after_a_rollups_read_cancels_it() {
    let (lists, probe) = probe_lists();
    let k = key("pause-late", 1);
    commit_edges_probe(&lists, &k);
    let inside = Arc::clone(&lists);
    probe.on_next_snapshot(Box::new(move || inside.pause_rollups()));
    assert_eq!(
        lists.rollup_key(&k).unwrap_err().0,
        PostingErrorEnum::HighPriorityOp
    );
    assert!(
        probe
            .inner()
            .contents()
            .versions(&k)
            .iter()
            .all(|v| v.meta() != BIT_COMPLETE_POSTING)
    );
}

/// Commits `uid 9` to `k` as one request: the commit's result.
fn commit_edges_result(lists: &TestLists, k: &[u8]) -> Result<Vec<u64>, PostingError> {
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request.get(k)?.add_posting(set_uid(9), false);
    batch.push(request);
    lists.commit(batch)
}

/// `n` keys with one delta each, written straight to the store so that nothing queues them.
/// `n` keys of `name`, each given a delta at 2 straight in `store`.
fn keys_with_deltas(store: &ultragraphdb_store_sim::SimStore, name: &str, n: u64) -> Vec<Vec<u8>> {
    (1..=n)
        .map(|i| {
            let k = key(name, i);
            write_delta(store, &k, 2, &[set_uid(1)]);
            k
        })
        .collect()
}

#[test]
fn test_a_low_priority_batch_is_taken_once_it_holds_sixteen_keys() {
    // ROLLUP_BATCH is 16: fifteen low-priority keys wait, the sixteenth hands the batch to the worker.
    let store = crate::utils::helpers::new_store();
    let keys = keys_with_deltas(&store, "low-batch", ROLLUP_BATCH as u64);
    let lists = crate::utils::helpers::lists_over(
        store,
        ultragraphdb_store_posting::ListsConfig::default(),
    );
    keys[..ROLLUP_BATCH - 1]
        .iter()
        .for_each(|k| lists.add_rollup_key(k.clone(), 1));
    assert_eq!(lists.run_rollups(100).unwrap(), 0);
    // Any priority above 0 is low.
    lists.add_rollup_key(keys[ROLLUP_BATCH - 1].clone(), 7);
    assert_eq!(lists.rollups_waiting(), 16);
    assert_eq!(lists.run_rollups(100).unwrap(), 16);
    assert_eq!(lists.rollups_waiting(), 0);
}

#[test]
fn test_a_purge_of_the_rolled_keys_keeps_those_of_the_last_ten_seconds() {
    // The purge runs every 300 seconds; a key rolled up 5 seconds before it is still not due.
    let lists = new_lists();
    let k = key("purge", 1);
    commit_edges(&lists, &k, &[set_uid(1)]);
    assert_eq!(lists.run_rollups(295).unwrap(), 1);
    commit_edges(&lists, &k, &[set_uid(2)]);
    assert_eq!(
        lists.run_rollups(300).unwrap(),
        0,
        "rolled up five seconds before the purge"
    );
    lists.add_rollup_key(k.clone(), 0);
    assert_eq!(
        lists.run_rollups(305).unwrap(),
        1,
        "ten seconds after its rollup"
    );
}
