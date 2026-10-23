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
        .add_mutation(&set_uid(1), None)
        .unwrap();
    batch.push(first);
    let mut second = batch.request(lists.latest_ts());
    second
        .get(&b)
        .unwrap()
        .add_mutation(&set_uid(2), None)
        .unwrap();
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
            .add_mutation(&set_uid(1), None)
            .unwrap();
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
        .add_mutation(&set_uid(1), None)
        .unwrap();
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
    let first = commit_edges(&lists, &k, &[set_uid(1)], None);
    lists.read(&k, first).unwrap();
    let second = commit_edges(&lists, &k, &[set_uid(2)], None);
    let entry = lists.cache().get(&k).expect("replaced");
    assert_eq!(entry.ts(), second);
    assert_eq!(uids(entry.list(), second, 0), vec![1, 2]);
}

#[test]
fn test_a_failed_commit_stops_writes_and_the_reads_above_it() {
    let lists = new_lists();
    let k = key("failed", 1);
    let ok = commit_edges(&lists, &k, &[set_uid(1)], None);
    lists.store().inject(Fault::FailNextBatch);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request
        .get(&k)
        .unwrap()
        .add_mutation(&set_uid(2), None)
        .unwrap();
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
    let ts = commit_edges(&reopened, &k, &[set_uid(3)], None);
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
        // While the batch at commit_ts is written: a read at commit_ts + 1 must wait, one below may go ahead,
        // and the store does not hold the batch yet.
        let waits = !inside.try_gate(commit_ts + 1).unwrap();
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
        .add_mutation(&set_uid(1), None)
        .unwrap();
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
    let ts = commit_edges(&lists, &k, &[set_uid(1)], None);
    lists.cache().remove(&k);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    request
        .get(&key("inside", 2))
        .unwrap()
        .add_mutation(&set_uid(2), None)
        .unwrap();
    batch.push(request);
    let results = lists
        .commit_with(batch, |_| {
            let read = lists.read(&k, ts)?;
            assert_eq!(uids(&read, ts, 0), vec![1]);
            Ok::<_, PostingError>(())
        })
        .unwrap();
    assert!(results[0].is_ok());
    assert!(
        lists.cache().get(&k).is_none(),
        "the fill found the writer lock held"
    );
}

#[test]
fn test_a_failed_commit_is_reported_as_an_error() {
    let capture = ultragraphdb_store_sim::LogCapture::new();
    tracing::subscriber::with_default(capture.subscriber(), || {
        let lists = new_lists();
        lists.store().inject(Fault::FailNextBatch);
        let mut batch = lists.batch();
        let mut request = batch.request(lists.latest_ts());
        request
            .get(&key("reported", 1))
            .unwrap()
            .add_mutation(&set_uid(1), None)
            .unwrap();
        batch.push(request);
        assert!(lists.commit(batch).is_err());
    });
    let events = capture.events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].level(), tracing::Level::ERROR);
    assert!(events[0].field("first_commit_ts").is_some());
    assert!(events[0].field("error").is_some());
}
