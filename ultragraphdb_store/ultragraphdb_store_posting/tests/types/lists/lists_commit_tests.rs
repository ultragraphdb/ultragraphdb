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
fn test_requests_of_a_batch_take_successive_even_versions_in_one_store_batch() {
    let lists = new_lists();
    let (a, b) = (key("batch", 1), key("batch", 2));
    let mut batch = lists.batch();
    let mut first = batch.request(lists.applied_ts());
    first
        .get(&a)
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    batch.push(first);
    let mut second = batch.request(lists.applied_ts());
    second
        .get(&b)
        .unwrap()
        .add_posting(set_uid(2).clone(), false);
    batch.push(second);
    let next = lists.next_version();
    let ts = lists.commit(batch).unwrap();
    // Even versions two apart: a rollup's version, one above the newest commit it folds, falls between them.
    assert_eq!((next, ts.clone()), (2, vec![2, 4]));
    assert_eq!(lists.applied_ts(), 4);
    assert_eq!(lists.store().applied_ts(), 4);
    let contents = lists.store().contents();
    assert_eq!(contents.versions(&a)[0].version(), 2);
    assert_eq!(contents.versions(&b)[0].version(), 4);
}

#[test]
fn test_a_request_refused_at_commit_writes_nothing_and_takes_no_timestamp() {
    let lists = new_lists();
    let (a, b) = (key("refused", 1), key("refused", 2));
    let mut batch = lists.batch();
    for k in [&a, &b] {
        let mut request = batch.request(lists.applied_ts());
        request
            .get(k)
            .unwrap()
            .add_posting(set_uid(1).clone(), false);
        batch.push(request);
    }
    let next = lists.next_version();
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
    let mut request = batch.request(lists.applied_ts());
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
    let mut request = batch.request(lists.applied_ts());
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
    let mut request = batch.request(lists.applied_ts());
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
    let mut request = batch.request(lists.applied_ts());
    request
        .get(&k)
        .unwrap()
        .add_posting(set_uid(2).clone(), false);
    batch.push(request);
    let failed_at = lists.next_version();
    assert!(matches!(
        lists.commit(batch).unwrap_err().0,
        PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
    ));
    assert_eq!(lists.applied_ts(), ok, "a failed commit is not applied");

    let poisoned = PostingError::Store(StoreError::Poisoned());
    assert_eq!(lists.try_gate(failed_at), Err(poisoned.clone()));
    assert!(
        lists.try_gate(ok).unwrap(),
        "a read at the watermark goes ahead"
    );
    assert_eq!(
        lists.try_gate(ok + 1),
        Err(poisoned.clone()),
        "a read above the watermark is refused: nothing will take the watermark there"
    );
    assert_eq!(lists.rollup_key(&k), Err(poisoned.clone()));
    assert_eq!(lists.drop_all(Vec::new()), Err(poisoned));

    // Reopening: new lists over the recovered store.
    lists.store().recover();
    let reopened: TestLists = lists_over(lists.store().clone(), lists.config());
    let applied = reopened.store().applied_ts();
    let ts = commit_edges(&reopened, &k, &[set_uid(3)]);
    assert_eq!(ts, applied + 2);
    let read = reopened.read(&k, ts).unwrap();
    let mut expected = vec![1, 3];
    if applied >= failed_at {
        expected.insert(1, 2);
    }
    assert_eq!(uids(&read, ts, 0), expected);
}

/// While a round is written, a read at or above its first version waits, and one at the watermark goes ahead without
/// seeing it; once it is applied, a read at its version sees it.
#[test]
fn test_a_read_waits_for_a_round_below_its_timestamp() {
    let (lists, probe) = probe_lists();
    let k = key("gate", 1);
    let watermark =
        crate::utils::helpers::commit_edges_at(&*lists, &key("gate", 2), &[set_uid(9)], 6);
    let version = 10;
    let inside = Arc::clone(&lists);
    let observed = Arc::new(std::sync::Mutex::new(None));
    let seen = Arc::clone(&observed);
    let inner = probe.inner().clone();
    probe.on_next_apply(Box::new(move || {
        let waits = !inside.try_gate(version).unwrap()
            && !inside.try_gate(version - 1).unwrap()
            && !inside.try_gate(watermark + 1).unwrap();
        let at_watermark = inside.try_gate(watermark).unwrap();
        let held = inner
            .snapshot(version)
            .unwrap()
            .get(&key("gate", 1))
            .unwrap()
            .is_some();
        *seen.lock().unwrap() = Some((waits, at_watermark, held));
    }));
    let mut request = lists.request(lists.applied_ts());
    request.get(&k).unwrap().add_posting(set_uid(1), false);
    let writes = request.into_writes();
    lists.commit_round(&[(version, &writes)]).unwrap();
    assert_eq!(*observed.lock().unwrap(), Some((true, true, false)));
    assert!(lists.try_gate(version).unwrap());
    assert_eq!(uids(&lists.read(&k, version).unwrap(), version, 0), vec![1]);
}

/// A read at `u64::MAX` reads everything applied: it waits for nothing when no write is under way, and for the round
/// being written when one is.
#[test]
fn test_a_read_of_everything_waits_only_for_a_round_being_written() {
    let (lists, probe) = probe_lists();
    assert!(lists.try_gate(u64::MAX).unwrap());
    let inside = Arc::clone(&lists);
    let observed = Arc::new(std::sync::Mutex::new(None));
    let seen = Arc::clone(&observed);
    probe.on_next_apply(Box::new(move || {
        *seen.lock().unwrap() = Some(inside.try_gate(u64::MAX).unwrap());
    }));
    crate::utils::helpers::commit_edges_at(&*lists, &key("all", 1), &[set_uid(1)], 4);
    assert_eq!(*observed.lock().unwrap(), Some(false));
    assert!(lists.try_gate(u64::MAX).unwrap());
}

/// A round is one store batch: every transaction at its version, the last recorded as applied, buffered, never synced.
#[test]
fn test_a_round_writes_each_transaction_at_its_version_in_one_buffered_batch() {
    use ultragraphdb_store_sim::OpRecord;
    use ultragraphdb_traits_store::PersistMode;
    let lists = lists_over(
        crate::utils::helpers::new_store(),
        ultragraphdb_store_posting::ListsConfig::new(PersistMode::Sync, 1 << 20),
    );
    let (a, b) = (key("round", 1), key("round", 2));
    let writes: Vec<_> = [&a, &b]
        .iter()
        .map(|k| {
            let mut request = lists.request(0);
            request.get(k).unwrap().add_posting(set_uid(7), false);
            request.add_entry(Entry::deletion(
                [b"\x01".as_slice(), k.as_slice()].concat(),
                0,
            ));
            request.into_writes()
        })
        .collect();
    lists.store().take_log();
    lists
        .commit_round(&[(8, &writes[0]), (12, &writes[1])])
        .unwrap();
    let log = lists.store().take_log();
    let applied: Vec<_> = log
        .iter()
        .filter_map(|r| match r {
            OpRecord::Applied { mode, .. } => Some(*mode),
            _ => None,
        })
        .collect();
    assert_eq!(
        applied,
        vec![PersistMode::Buffer],
        "one batch, buffered whatever the configuration"
    );
    assert!(!log.contains(&OpRecord::Synced));
    let contents = lists.store().contents();
    assert_eq!(contents.versions(&a)[0].version(), 8);
    assert_eq!(contents.versions(&b)[0].version(), 12);
    assert_eq!(
        contents.versions(&[b"\x01".as_slice(), a.as_slice()].concat())[0].version(),
        8
    );
    assert_eq!(
        contents.versions(&[b"\x01".as_slice(), b.as_slice()].concat())[0].version(),
        12
    );
    assert_eq!((lists.applied_ts(), contents.applied_ts()), (12, 12));
    assert_eq!(
        lists.rollups_waiting(),
        4,
        "each key written, at both priorities"
    );
}

/// A round writes each transaction's entries at their own versions, its versionless writes and its record removals,
/// and advances the epoch when one of its transactions asks.
#[test]
fn test_a_round_writes_every_kind_of_write() {
    use ultragraphdb_traits_store::RebuildRecord;
    let lists = new_lists();
    lists
        .store()
        .write_rebuild_record(&RebuildRecord::new(3, vec![vec![0x00]]))
        .unwrap();
    let epoch = lists.epoch();
    let mut request = lists.request(0);
    request.add_entry_at_version(Entry::deletion(b"\x01v".to_vec(), 1));
    request.add_versionless(b"term".to_vec(), b"uid".to_vec());
    request.remove_rebuild_record_on_commit(3);
    let first = request.into_writes();
    let mut request = lists.request(0);
    request.advance_epoch_on_commit();
    let second = request.into_writes();
    lists.commit_round(&[(2, &first), (4, &second)]).unwrap();
    let contents = lists.store().contents();
    assert_eq!(contents.versions(b"\x01v")[0].version(), 1);
    assert_eq!(
        lists.store().get_versionless(b"term").unwrap(),
        Some(b"uid".to_vec())
    );
    assert!(lists.store().rebuild_records().unwrap().is_empty());
    assert_eq!(lists.epoch(), epoch + 1);
}

/// A round updates the cache entries of the lists it writes, each at its transaction's version.
#[test]
fn test_a_round_replaces_the_cached_entries_at_their_versions() {
    let lists = new_lists();
    let k = key("round-cache", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.read(&k, ts).unwrap();
    crate::utils::helpers::commit_edges_at(&lists, &k, &[set_uid(2)], 10);
    let entry = lists.cache().get(&k).expect("replaced");
    assert_eq!(entry.ts(), 10);
    assert_eq!(uids(entry.list(), 10, 0), vec![1, 2]);
}

/// An empty round writes nothing and moves nothing.
#[test]
fn test_an_empty_round_writes_nothing() {
    let lists = new_lists();
    lists.store().take_log();
    lists.commit_round(&[]).unwrap();
    assert!(lists.store().take_log().is_empty());
    assert_eq!(lists.applied_ts(), 0);
}

/// A version at or below the watermark, or not above the round's previous one, is refused naming both, and nothing of
/// the round is written.
#[test]
fn test_a_round_with_a_stale_version_is_refused_whole() {
    let lists = new_lists();
    let ts = crate::utils::helpers::commit_edges_at(&lists, &key("stale", 1), &[set_uid(1)], 6);
    let mut request = lists.request(ts);
    request
        .get(&key("stale", 2))
        .unwrap()
        .add_posting(set_uid(2), false);
    let writes = request.into_writes();
    let before = lists.store().contents();
    for (round, refused) in [
        (vec![(6, &writes)], PostingError::StaleVersion(6, 6)),
        (vec![(4, &writes)], PostingError::StaleVersion(4, 6)),
        (
            vec![(8, &writes), (8, &writes)],
            PostingError::StaleVersion(8, 8),
        ),
        (
            vec![(10, &writes), (8, &writes)],
            PostingError::StaleVersion(8, 10),
        ),
    ] {
        assert_eq!(lists.commit_round(&round), Err(refused));
        assert_eq!(lists.store().contents(), before);
        assert_eq!(lists.applied_ts(), 6);
    }
    lists.commit_round(&[(7, &writes)]).unwrap();
    assert_eq!(
        lists.applied_ts(),
        7,
        "any version above the watermark is taken"
    );
}

/// A round that fails to write stops every later write and refuses the reads at or above its first version; reads at
/// the watermark go ahead.
#[test]
fn test_a_failed_round_stops_writes_and_the_reads_above_it() {
    let lists = new_lists();
    let ok =
        crate::utils::helpers::commit_edges_at(&lists, &key("round-fail", 1), &[set_uid(1)], 4);
    lists.store().inject(Fault::FailNextBatch);
    let mut request = lists.request(ok);
    request
        .get_from_delta(&key("round-fail", 2))
        .add_posting(set_uid(2), false);
    let writes = request.into_writes();
    assert!(matches!(
        lists.commit_round(&[(8, &writes)]).unwrap_err().0,
        PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
    ));
    let poisoned = PostingError::Store(StoreError::Poisoned());
    assert_eq!(lists.applied_ts(), ok);
    assert_eq!(lists.try_gate(8), Err(poisoned.clone()));
    assert!(lists.try_gate(ok).unwrap());
    assert_eq!(lists.commit_round(&[(10, &writes)]), Err(poisoned));
}

#[test]
fn test_a_read_inside_a_commits_check_neither_deadlocks_nor_fills() {
    let lists = new_lists();
    let k = key("inside", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.cache().remove(&k);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request
        .get(&key("inside", 2))
        .unwrap()
        .add_posting(set_uid(2).clone(), false);
    batch.push(request);
    let next = lists.next_version();
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
        let mut request = batch.request(lists.applied_ts());
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
    let mut request = batch.request(lists.applied_ts());
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
    let mut request = batch.request(lists.applied_ts());
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
    let mut request = batch.request(lists.applied_ts());
    request.add_entry_at_version(Entry::deletion(k.clone(), 1));
    request.remove_rebuild_record_on_commit(4);
    batch.push(request);
    let mut refused = batch.request(lists.applied_ts());
    refused.remove_rebuild_record_on_commit(5);
    batch.push(refused);
    let next = lists.next_version();
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
    let mut request = batch.request(lists.applied_ts());
    request.get(k).unwrap().add_posting(set_uid(1), false);
    if advance {
        request.advance_epoch_on_commit();
    }
    batch.push(request);
}

#[test]
fn test_the_requests_kept_take_successive_versions_around_one_refused() {
    let lists = new_lists();
    let keys: Vec<_> = (1..=3).map(|i| key("around", i)).collect();
    let mut batch = lists.batch();
    keys.iter()
        .for_each(|k| push_write(&mut batch, &lists, k, false));
    let next = lists.next_version();
    let mut seen = 0;
    let results = lists
        .commit_with(batch, |_| {
            seen += 1;
            if seen == 2 { Err("refused") } else { Ok(()) }
        })
        .unwrap();
    assert_eq!(results, vec![Ok(next), Err("refused"), Ok(next + 2)]);
    assert_eq!(
        (lists.applied_ts(), lists.next_version()),
        (next + 2, next + 4)
    );
    let contents = lists.store().contents();
    assert_eq!(contents.versions(&keys[2])[0].version(), next + 2);
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
    let mut request = batch.request(lists.applied_ts());
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
    let mut request = batch.request(lists.applied_ts());
    request.get_from_delta(&k).add_posting(set_uid(2), false);
    batch.push(request);
    lists.commit(batch).unwrap();
    assert!(lists.cache().get(&k).is_none());
}

#[test]
fn test_an_empty_batch_commits_nothing() {
    let lists = new_lists();
    let next = lists.next_version();
    assert_eq!(lists.commit(lists.batch()).unwrap(), Vec::<u64>::new());
    assert_eq!((lists.next_version(), lists.applied_ts()), (next, 0));
}

#[test]
fn test_a_commit_queues_each_written_key_at_both_priorities_and_no_read_key() {
    let lists = new_lists();
    let (written, read) = (key("queued", 1), key("queued", 2));
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
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
