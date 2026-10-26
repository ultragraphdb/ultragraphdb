/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::{BIT_SCHEMA_POSTING, PostingError, PostingErrorEnum};
use ultragraphdb_store_sim::Fault;
use ultragraphdb_traits_store::{Entry, ListCache, Store, StoreSnapshot};

use crate::utils::helpers::{
    TestLists, commit_edges, key, lists_over, new_lists, probe_lists, set_uid, uids,
};

#[test]
fn test_requests_of_a_batch_take_consecutive_timestamps_in_one_store_batch() {
    let lists = new_lists();
    let (a, b) = (key("batch", 1), key("batch", 2));
    let mut batch = lists.batch();
    let mut first = batch.request(lists.latest_ts());
    first
        .get(&a)
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    batch.push(first);
    let mut second = batch.request(lists.latest_ts());
    second
        .get(&b)
        .unwrap()
        .add_posting(set_uid(2).clone(), false);
    batch.push(second);
    let next = lists.peek_ts();
    let ts = lists.commit(batch).unwrap();
    assert_eq!(ts, vec![next, next + 1]);
    assert_eq!(lists.applied_ts(), next + 1);
    assert_eq!(lists.store().applied_ts(), next + 1);
    let contents = lists.store().contents();
    assert_eq!(contents.versions(&a)[0].version(), next);
    assert_eq!(contents.versions(&b)[0].version(), next + 1);
}

#[test]
fn test_a_request_refused_at_commit_writes_nothing_and_takes_no_timestamp() {
    let lists = new_lists();
    let (a, b) = (key("refused", 1), key("refused", 2));
    let mut batch = lists.batch();
    for k in [&a, &b] {
        let mut request = batch.request(lists.latest_ts());
        request
            .get(k)
            .unwrap()
            .add_posting(set_uid(1).clone(), false);
        batch.push(request);
    }
    let next = lists.peek_ts();
    let results = lists
        .commit_with(batch, |request| {
            if request.lists().any(|(k, _)| k == a.as_slice()) {
                Err("refused")
            } else {
                Ok(())
            }
        })
        .unwrap();
    assert_eq!(results, vec![Err("refused"), Ok(next)]);
    assert!(lists.store().contents().versions(&a).is_empty());
    assert_eq!(lists.applied_ts(), next);
}

#[test]
fn test_a_batch_refused_whole_writes_nothing() {
    let lists = new_lists();
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request
        .get(&key("none", 1))
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    batch.push(request);
    let applied = lists.applied_ts();
    let results = lists.commit_with(batch, |_| Err(())).unwrap();
    assert_eq!(results, vec![Err(())]);
    assert_eq!(lists.applied_ts(), applied);
    assert!(lists.store().contents().keys().is_empty());
}

#[test]
fn test_a_requests_entries_are_written_at_its_commit_timestamp() {
    let lists = new_lists();
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request.add_entry(
        Entry::put(b"\x01schema".to_vec(), 0, BIT_SCHEMA_POSTING, b"s".to_vec()).with_discard(),
    );
    request.add_entry(Entry::deletion(b"\x02type".to_vec(), 0));
    batch.push(request);
    let ts = lists.commit(batch).unwrap()[0];
    let contents = lists.store().contents();
    let schema = &contents.versions(b"\x01schema")[0];
    assert_eq!((schema.version(), schema.discard()), (ts, true));
    assert_eq!(contents.versions(b"\x02type")[0].version(), ts);
}

#[test]
fn test_a_list_without_postings_is_not_written() {
    let lists = new_lists();
    let k = key("empty-delta", 1);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request.get(&k).unwrap();
    batch.push(request);
    lists.commit(batch).unwrap();
    assert!(lists.store().contents().versions(&k).is_empty());
}

#[test]
fn test_a_commit_replaces_the_entry_of_a_cached_list() {
    let lists = new_lists();
    let k = key("replace", 1);
    let first = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.read(&k, first).unwrap();
    let second = commit_edges(&lists, &k, &[set_uid(2)]);
    let entry = lists.cache().get(&k).expect("replaced");
    assert_eq!(entry.ts(), second);
    assert_eq!(uids(entry.list(), second, 0), vec![1, 2]);
}

#[test]
fn test_a_failed_commit_stops_writes_and_the_reads_above_it() {
    let lists = new_lists();
    let k = key("failed", 1);
    let ok = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.store().inject(Fault::FailNextBatch);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request
        .get(&k)
        .unwrap()
        .add_posting(set_uid(2).clone(), false);
    batch.push(request);
    let failed_at = lists.peek_ts();
    assert!(matches!(
        lists.commit(batch).unwrap_err().0,
        PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
    ));
    assert_eq!(lists.applied_ts(), ok, "a failed commit is not applied");

    let poisoned = PostingError::Store(StoreError::Poisoned());
    assert_eq!(lists.try_gate(failed_at), Err(poisoned.clone()));
    assert!(
        lists.try_gate(failed_at - 1).unwrap(),
        "reads below the failed batch go ahead"
    );
    assert_eq!(lists.rollup_key(&k), Err(poisoned.clone()));
    assert_eq!(lists.drop_all(Vec::new()), Err(poisoned));

    // Reopening: new lists over the recovered store.
    lists.store().recover();
    let reopened: TestLists = lists_over(lists.store().clone(), lists.config());
    let applied = reopened.store().applied_ts();
    let ts = commit_edges(&reopened, &k, &[set_uid(3)]);
    assert!(ts > applied + 1);
    let read = reopened.read(&k, ts).unwrap();
    let mut expected = vec![1, 3];
    if applied >= failed_at {
        expected.insert(1, 2);
    }
    assert_eq!(uids(&read, ts, 0), expected);
}

#[test]
fn test_a_read_waits_for_a_commit_below_its_timestamp() {
    let (lists, probe) = probe_lists();
    let k = key("gate", 1);
    let commit_ts = lists.peek_ts();
    let inside = Arc::clone(&lists);
    let observed = Arc::new(std::sync::Mutex::new(None));
    let seen = Arc::clone(&observed);
    let inner = probe.inner().clone();
    probe.on_next_apply(Box::new(move || {
        // While the batch at commit_ts is written: reads at commit_ts and above must wait, one below may go ahead,
        // and the store does not hold the batch yet.
        let waits =
            !inside.try_gate(commit_ts + 1).unwrap() && !inside.try_gate(commit_ts).unwrap();
        let below = inside.try_gate(commit_ts - 1).unwrap();
        let held = inner
            .snapshot(commit_ts + 1)
            .unwrap()
            .get(&key("gate", 1))
            .unwrap()
            .is_some();
        *seen.lock().unwrap() = Some((waits, below, held));
    }));
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request
        .get(&k)
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    batch.push(request);
    assert_eq!(lists.commit(batch).unwrap(), vec![commit_ts]);
    assert_eq!(*observed.lock().unwrap(), Some((true, true, false)));
    assert!(lists.try_gate(commit_ts + 1).unwrap());
    assert_eq!(
        uids(&lists.read(&k, commit_ts + 1).unwrap(), commit_ts + 1, 0),
        vec![1]
    );
}

#[test]
fn test_a_read_inside_a_commits_check_neither_deadlocks_nor_fills() {
    let lists = new_lists();
    let k = key("inside", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.cache().remove(&k);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request
        .get(&key("inside", 2))
        .unwrap()
        .add_posting(set_uid(2).clone(), false);
    batch.push(request);
    let next = lists.peek_ts();
    let results = lists
        .commit_with(batch, |_| {
            let read = lists.read(&k, ts)?;
            assert_eq!(uids(&read, ts, 0), vec![1]);
            Ok::<_, PostingError>(())
        })
        .unwrap();
    assert_eq!(results, vec![Ok(next)]);
    assert!(
        lists.cache().get(&k).is_none(),
        "the fill found the writer lock held"
    );
}

#[test]
fn test_a_failed_commit_is_reported_as_an_error() {
    let capture = ultragraphdb_store_sim::LogCapture::new();
    capture.capture(|| {
        let lists = new_lists();
        lists.store().inject(Fault::FailNextBatch);
        let mut batch = lists.batch();
        let mut request = batch.request(lists.latest_ts());
        request
            .get(&key("reported", 1))
            .unwrap()
            .add_posting(set_uid(1).clone(), false);
        batch.push(request);
        assert!(matches!(
            lists.commit(batch).unwrap_err().0,
            PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
        ));
    });
    let events = capture.events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].level(), tracing::Level::ERROR);
    assert!(events[0].field("first_commit_ts").is_some());
    assert!(events[0].field("error").is_some());
}

/// An entry added with its version kept is written at that version, the request's other writes at its commit
/// timestamp: what a rebuild's swap writes, its built lists at the rebuild's timestamp.
#[test]
fn test_an_entry_at_its_own_version_keeps_it() {
    let lists = new_lists();
    let (a, b) = (key("versioned", 1), key("versioned", 2));
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request.add_entry_at_version(Entry::deletion(a.clone(), 1));
    request.add_entry(Entry::deletion(b.clone(), 1));
    batch.push(request);
    let ts = lists.commit(batch).unwrap()[0];
    let contents = lists.store().contents();
    assert_eq!(contents.versions(&a)[0].version(), 1);
    assert_eq!(contents.versions(&b)[0].version(), ts);
}

/// A request marked to advance the epoch does so under the writer lock with its batch, so no cached list read
/// before the batch serves a read after it.
#[test]
fn test_a_commit_that_advances_the_epoch_leaves_no_older_entry_serving() {
    let lists = new_lists();
    let k = key("epoch", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.read(&k, ts).unwrap();
    assert!(lists.cached(&k, ts).unwrap().is_some());
    let epoch = lists.epoch();
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request.add_entry_at_version(Entry::deletion(k.clone(), ts));
    request.advance_epoch_on_commit();
    batch.push(request);
    let swapped = lists.commit(batch).unwrap()[0];
    assert_eq!(lists.epoch(), epoch + 1);
    assert!(lists.cached(&k, swapped).unwrap().is_none());
    assert!(uids(&lists.read(&k, swapped).unwrap(), swapped, 0).is_empty());
}

/// A request marked to remove a rebuild record removes it in its commit batch, atomically with its writes; the
/// record of a request left out stays.
#[test]
fn test_a_commit_removes_the_rebuild_records_its_requests_name() {
    use ultragraphdb_traits_store::RebuildRecord;
    let lists = new_lists();
    let store = lists.store();
    store
        .write_rebuild_record(&RebuildRecord::new(4, vec![vec![0x00]]))
        .unwrap();
    store
        .write_rebuild_record(&RebuildRecord::new(5, vec![vec![0x00]]))
        .unwrap();
    let k = key("swap", 1);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request.add_entry_at_version(Entry::deletion(k.clone(), 1));
    request.remove_rebuild_record_on_commit(4);
    batch.push(request);
    let mut refused = batch.request(lists.latest_ts());
    refused.remove_rebuild_record_on_commit(5);
    batch.push(refused);
    let next = lists.peek_ts();
    let mut seen = 0;
    let results = lists
        .commit_with(batch, |_| {
            seen += 1;
            if seen == 2 { Err("refused") } else { Ok(()) }
        })
        .unwrap();
    assert_eq!(results, vec![Ok(next), Err("refused")]);
    let records: Vec<u64> = store
        .rebuild_records()
        .unwrap()
        .iter()
        .map(|r| r.id())
        .collect();
    assert_eq!(records, [5]);
}

/// Pushes a request writing uid 1 to `k` into `batch`, marked to advance the epoch when `advance` is set.
fn push_write(
    batch: &mut ultragraphdb_store_posting::Batch<
        '_,
        ultragraphdb_store_sim::SimStore,
        ultragraphdb_store_sim::SimListCache<ultragraphdb_store_posting::CachedList>,
    >,
    lists: &TestLists,
    k: &[u8],
    advance: bool,
) {
    let mut request = batch.request(lists.latest_ts());
    request.get(k).unwrap().add_posting(set_uid(1), false);
    if advance {
        request.advance_epoch_on_commit();
    }
    batch.push(request);
}

#[test]
fn test_the_requests_kept_take_consecutive_timestamps_around_one_refused() {
    let lists = new_lists();
    let keys: Vec<_> = (1..=3).map(|i| key("around", i)).collect();
    let mut batch = lists.batch();
    keys.iter()
        .for_each(|k| push_write(&mut batch, &lists, k, false));
    let next = lists.peek_ts();
    let mut seen = 0;
    let results = lists
        .commit_with(batch, |_| {
            seen += 1;
            if seen == 2 { Err("refused") } else { Ok(()) }
        })
        .unwrap();
    assert_eq!(results, vec![Ok(next), Err("refused"), Ok(next + 1)]);
    assert_eq!((lists.applied_ts(), lists.peek_ts()), (next + 1, next + 2));
    let contents = lists.store().contents();
    assert_eq!(contents.versions(&keys[2])[0].version(), next + 1);
    assert!(contents.versions(&keys[1]).is_empty());
}

#[test]
fn test_a_refused_request_does_not_advance_the_epoch() {
    let lists = new_lists();
    let epoch = lists.epoch();
    let mut batch = lists.batch();
    push_write(&mut batch, &lists, &key("refused-epoch", 1), true);
    push_write(&mut batch, &lists, &key("refused-epoch", 2), false);
    let mut seen = 0;
    lists
        .commit_with(batch, |_| {
            seen += 1;
            if seen == 1 { Err(()) } else { Ok(()) }
        })
        .unwrap();
    assert_eq!(lists.epoch(), epoch);
}

#[test]
fn test_a_commit_removes_a_cached_list_of_an_older_epoch() {
    let lists = new_lists();
    let k = key("older-epoch", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.read(&k, ts).unwrap();
    lists.advance_epoch();
    assert!(
        lists.cache().get(&k).is_some(),
        "an older entry lingers until something removes it"
    );
    // Written blind, so that no read refills the entry first.
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request.get_from_delta(&k).add_posting(set_uid(2), false);
    batch.push(request);
    lists.commit(batch).unwrap();
    assert!(lists.cache().get(&k).is_none());
}

#[test]
fn test_a_commit_leaves_an_uncached_list_uncached() {
    let lists = new_lists();
    let k = key("uncached", 1);
    commit_edges(&lists, &k, &[set_uid(1)]);
    lists.cache().remove(&k);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request.get_from_delta(&k).add_posting(set_uid(2), false);
    batch.push(request);
    lists.commit(batch).unwrap();
    assert!(lists.cache().get(&k).is_none());
}

#[test]
fn test_an_empty_batch_commits_nothing() {
    let lists = new_lists();
    let next = lists.peek_ts();
    assert_eq!(lists.commit(lists.batch()).unwrap(), Vec::<u64>::new());
    assert_eq!((lists.peek_ts(), lists.applied_ts()), (next, 0));
}

#[test]
fn test_a_commit_queues_each_written_key_at_both_priorities_and_no_read_key() {
    let lists = new_lists();
    let (written, read) = (key("queued", 1), key("queued", 2));
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request.get(&read).unwrap();
    request
        .get(&written)
        .unwrap()
        .add_posting(set_uid(1), false);
    batch.push(request);
    lists.commit(batch).unwrap();
    assert_eq!(lists.rollups_waiting(), 2);
}

/// The lists refuse every write after a failed one on their own, even where the store would take it: a recovered
/// store is reopened with new lists, never written through the old ones.
#[test]
fn test_lists_refuse_writes_after_a_failure_whatever_the_store_says() {
    let lists = new_lists();
    let k = key("refuse", 1);
    let ok = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.store().inject(Fault::FailNextBatch);
    let mut batch = lists.batch();
    let mut request = batch.request(ok);
    request.get_from_delta(&k).add_posting(set_uid(2), false);
    batch.push(request);
    assert!(lists.commit(batch).is_err());
    lists.store().recover();
    let before = lists.store().contents();
    let poisoned = PostingError::Store(StoreError::Poisoned());
    let mut batch = lists.batch();
    let mut request = batch.request(ok);
    request.get_from_delta(&k).add_posting(set_uid(3), false);
    batch.push(request);
    assert_eq!(lists.commit(batch), Err(poisoned.clone()));
    assert_eq!(lists.drop_all(Vec::new()), Err(poisoned.clone()));
    assert_eq!(
        lists.drop_prefixes(ok + 50, std::slice::from_ref(&k), Vec::new()),
        Err(poisoned.clone())
    );
    assert_eq!(
        lists.drop_prefixes_now(std::slice::from_ref(&k), Vec::new()),
        Err(poisoned.clone())
    );
    assert_eq!(lists.drop_all_now(Vec::new()), Err(poisoned));
    assert_eq!(lists.store().contents(), before, "nothing written");
}
