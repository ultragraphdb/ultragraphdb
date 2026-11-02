/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::{Arc, PoisonError};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::{BIT_SCHEMA_POSTING, PostingError, PostingErrorEnum};
use ultragraphdb_store_sim::Fault;
use ultragraphdb_traits_store::{Entry, ListCache, Store, StoreSnapshot, VersionlessEntry};

use crate::utils::helpers::{
    Failure, TestLists, commit, commit_edges, key, lists_over, new_lists, probe_lists, set_uid,
    uids,
};

#[test]
fn test_requests_of_a_batch_take_successive_even_versions_in_one_store_batch() -> TestResult {
    let lists = new_lists();
    let (a, b) = (key("batch", 1), key("batch", 2));
    let mut batch = lists.batch();
    let mut first = batch.request(lists.applied_ts());
    first.get(&a)?.add_posting(set_uid(1).clone(), false);
    batch.push(first);
    let mut second = batch.request(lists.applied_ts());
    second.get(&b)?.add_posting(set_uid(2).clone(), false);
    batch.push(second);
    let next = lists.next_version();
    let ts = lists.commit(batch)?;
    // Even versions two apart: a rollup's version, one above the newest commit it folds, falls between them.
    assert_eq!((next, ts.clone()), (2, vec![2, 4]));
    assert_eq!(lists.applied_ts(), 4);
    assert_eq!(lists.store().applied_ts(), 4);
    let contents = lists.store().contents();
    assert_eq!(contents.versions(&a)[0].version(), 2);
    assert_eq!(contents.versions(&b)[0].version(), 4);
    Ok(())
}

#[test]
fn test_a_request_refused_at_commit_writes_nothing_and_takes_no_timestamp() -> TestResult {
    let lists = new_lists();
    let (a, b) = (key("refused", 1), key("refused", 2));
    let mut batch = lists.batch();
    for k in [&a, &b] {
        let mut request = batch.request(lists.applied_ts());
        request.get(k)?.add_posting(set_uid(1).clone(), false);
        batch.push(request);
    }
    let next = lists.next_version();
    let results = lists.commit_with(batch, |request| {
        if request.lists().any(|(k, _)| k == a.as_slice()) {
            Err("refused")
        } else {
            Ok(())
        }
    })?;
    assert_eq!(results, vec![Err("refused"), Ok(next)]);
    assert!(lists.store().contents().versions(&a).is_empty());
    assert_eq!(lists.applied_ts(), next);
    Ok(())
}

#[test]
fn test_a_batch_refused_whole_writes_nothing() -> TestResult {
    let lists = new_lists();
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request
        .get(&key("none", 1))?
        .add_posting(set_uid(1).clone(), false);
    batch.push(request);
    let applied = lists.applied_ts();
    let results = lists.commit_with(batch, |_| Err(()))?;
    assert_eq!(results, vec![Err(())]);
    assert_eq!(lists.applied_ts(), applied);
    assert!(lists.store().contents().keys().is_empty());
    Ok(())
}

#[test]
fn test_a_requests_entries_are_written_at_its_commit_timestamp() -> TestResult {
    let lists = new_lists();
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request.add_entry(
        Entry::put(b"\x01schema".to_vec(), 0, BIT_SCHEMA_POSTING, b"s".to_vec()).with_discard(),
    );
    request.add_entry(Entry::deletion(b"\x02type".to_vec(), 0));
    batch.push(request);
    let ts = lists.commit(batch)?[0];
    let contents = lists.store().contents();
    let schema = &contents.versions(b"\x01schema")[0];
    assert_eq!((schema.version(), schema.discard()), (ts, true));
    assert_eq!(contents.versions(b"\x02type")[0].version(), ts);
    Ok(())
}

#[test]
fn test_a_list_without_postings_is_not_written() -> TestResult {
    let lists = new_lists();
    let k = key("empty-delta", 1);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    let _k_list: &mut ultragraphdb_store_posting::List = request.get(&k)?;
    batch.push(request);
    let _versions: Vec<u64> = lists.commit(batch)?;
    assert!(lists.store().contents().versions(&k).is_empty());
    Ok(())
}

#[test]
fn test_a_commit_replaces_the_entry_of_a_cached_list() -> TestResult {
    let lists = new_lists();
    let k = key("replace", 1);
    let first = commit_edges(&lists, &k, &[set_uid(1)])?;
    let _k_list: ultragraphdb_store_posting::List = lists.read(&k, first)?;
    let second = commit_edges(&lists, &k, &[set_uid(2)])?;
    let entry = lists.cache().get(&k).context("replaced")?;
    assert_eq!(entry.ts(), second);
    assert_eq!(uids(entry.list(), second, 0)?, vec![1, 2]);
    Ok(())
}

#[test]
fn test_a_failed_commit_stops_writes_and_the_reads_above_it() -> TestResult {
    let lists = new_lists();
    let k = key("failed", 1);
    let ok = commit_edges(&lists, &k, &[set_uid(1)])?;
    lists.store().inject(Fault::FailNextBatch);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request.get(&k)?.add_posting(set_uid(2).clone(), false);
    batch.push(request);
    let failed_at = lists.next_version();
    assert!(matches!(
        lists.commit(batch).failure("the commit")?.0,
        PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
    ));
    assert_eq!(lists.applied_ts(), ok, "a failed commit is not applied");

    let poisoned = PostingError::store(StoreError::poisoned());
    assert_eq!(lists.try_gate(failed_at), Err(poisoned.clone()));
    assert!(lists.try_gate(ok)?, "a read at the watermark goes ahead");
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
    let ts = commit_edges(&reopened, &k, &[set_uid(3)])?;
    assert_eq!(ts, applied + 2);
    let read = reopened.read(&k, ts)?;
    let mut expected = vec![1, 3];
    if applied >= failed_at {
        expected.insert(1, 2);
    }
    assert_eq!(uids(&read, ts, 0)?, expected);
    Ok(())
}

/// While a round is written, a read at or above its first version waits, and one at the watermark goes ahead without
/// seeing it; once it is applied, a read at its version sees it.
#[test]
fn test_a_read_waits_for_a_round_below_its_timestamp() -> TestResult {
    let (lists, probe) = probe_lists();
    let k = key("gate", 1);
    let watermark =
        crate::utils::helpers::commit_edges_at(&*lists, &key("gate", 2), &[set_uid(9)], 6)?;
    let version = 10;
    let inside = Arc::clone(&lists);
    let observed = Arc::new(std::sync::Mutex::new(None));
    let seen = Arc::clone(&observed);
    let inner = probe.inner().clone();
    probe.on_next_apply(Box::new(move || -> TestResult {
        let waits = !inside.try_gate(version)?
            && !inside.try_gate(version - 1)?
            && !inside.try_gate(watermark + 1)?;
        let at_watermark = inside.try_gate(watermark)?;
        let held = inner.snapshot(version)?.get(&key("gate", 1))?.is_some();
        *seen.lock().unwrap_or_else(PoisonError::into_inner) = Some((waits, at_watermark, held));
        Ok(())
    }));
    let mut request = lists.request(lists.applied_ts());
    request.get(&k)?.add_posting(set_uid(1), false);
    let writes = request.into_writes();
    lists.commit_round(&[(version, &writes)])?;
    assert_eq!(
        *observed.lock().unwrap_or_else(PoisonError::into_inner),
        Some((true, true, false))
    );
    assert!(lists.try_gate(version)?);
    assert_eq!(uids(&lists.read(&k, version)?, version, 0)?, vec![1]);
    Ok(())
}

/// A read at `u64::MAX` reads everything applied: it waits for nothing when no write is under way, and for the round
/// being written when one is.
#[test]
fn test_a_read_of_everything_waits_only_for_a_round_being_written() -> TestResult {
    let (lists, probe) = probe_lists();
    assert!(lists.try_gate(u64::MAX)?);
    let inside = Arc::clone(&lists);
    let observed = Arc::new(std::sync::Mutex::new(None));
    let seen = Arc::clone(&observed);
    probe.on_next_apply(Box::new(move || -> TestResult {
        *seen.lock().unwrap_or_else(PoisonError::into_inner) = Some(inside.try_gate(u64::MAX)?);
        Ok(())
    }));
    crate::utils::helpers::commit_at(&*lists, &key("all", 1), &[set_uid(1)], 4)?;
    assert_eq!(
        *observed.lock().unwrap_or_else(PoisonError::into_inner),
        Some(false)
    );
    assert!(lists.try_gate(u64::MAX)?);
    Ok(())
}

/// A round is one store batch: every transaction at its version, the last recorded as applied, buffered, never synced.
#[test]
fn test_a_round_writes_each_transaction_at_its_version_in_one_buffered_batch() -> TestResult {
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
            request.get(k)?.add_posting(set_uid(7), false);
            request.add_entry(Entry::deletion(
                [b"\x01".as_slice(), k.as_slice()].concat(),
                0,
            ));
            Ok(request.into_writes())
        })
        .collect::<TestResult<_>>()?;
    let _log: Vec<OpRecord> = lists.store().take_log();
    lists.commit_round(&[(8, &writes[0]), (12, &writes[1])])?;
    let log = lists.store().take_log();
    let applied: Vec<_> = log
        .iter()
        .filter_map(|r| {
            if let OpRecord::Applied { mode, .. } = r {
                Some(*mode)
            } else {
                None
            }
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
    Ok(())
}

/// A round writes each transaction's entries at their own versions, its versionless writes and its record removals,
/// and advances the epoch when one of its transactions asks.
#[test]
fn test_a_round_writes_every_kind_of_write() -> TestResult {
    use ultragraphdb_traits_store::RebuildRecord;
    let lists = new_lists();
    lists
        .store()
        .write_rebuild_record(&RebuildRecord::new(3, vec![vec![0x00]]))?;
    let epoch = lists.epoch();
    let mut request = lists.request(0);
    request.add_entry_at_version(Entry::deletion(b"\x01v".to_vec(), 1));
    request.add_versionless(b"term".to_vec(), b"uid".to_vec());
    request.remove_rebuild_record_on_commit(3);
    let first = request.into_writes();
    let mut advancing = lists.request(0);
    advancing.advance_epoch_on_commit();
    let second = advancing.into_writes();
    lists.commit_round(&[(2, &first), (4, &second)])?;
    let contents = lists.store().contents();
    assert_eq!(contents.versions(b"\x01v")[0].version(), 1);
    assert_eq!(
        lists.store().get_versionless(b"term")?,
        Some(b"uid".to_vec())
    );
    assert!(lists.store().rebuild_records()?.is_empty());
    assert_eq!(lists.epoch(), epoch + 1);
    Ok(())
}

/// A round updates the cache entries of the lists it writes, each at its transaction's version.
#[test]
fn test_a_round_replaces_the_cached_entries_at_their_versions() -> TestResult {
    let lists = new_lists();
    let k = key("round-cache", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)])?;
    let _k_list: ultragraphdb_store_posting::List = lists.read(&k, ts)?;
    crate::utils::helpers::commit_at(&lists, &k, &[set_uid(2)], 10)?;
    let entry = lists.cache().get(&k).context("replaced")?;
    assert_eq!(entry.ts(), 10);
    assert_eq!(uids(entry.list(), 10, 0)?, vec![1, 2]);
    Ok(())
}

/// An empty round writes nothing and moves nothing.
#[test]
fn test_an_empty_round_writes_nothing() -> TestResult {
    let lists = new_lists();
    let _log: Vec<ultragraphdb_store_sim::OpRecord> = lists.store().take_log();
    lists.commit_round(&[])?;
    assert!(lists.store().take_log().is_empty());
    assert_eq!(lists.applied_ts(), 0);
    Ok(())
}

/// A round writes the system's entries without versions in its batch, a removal among them, and records its last version
/// as applied though no execution took it: a round whose last decision, or whose every decision, is a session
/// registration (7.8, decided 2026-10-28). A request's own removal without a version is written too.
#[test]
fn test_a_round_writes_the_systems_entries_and_moves_past_its_last_version() -> TestResult {
    let lists = new_lists();
    let mut request = lists.request(0);
    request.add_versionless(b"gone".to_vec(), b"old".to_vec());
    let first = request.into_writes();
    lists.commit_round(&[(2, &first)])?;
    let mut removing_request = lists.request(2);
    removing_request.add_versionless_entry(VersionlessEntry::removal(b"gone".to_vec()));
    let removing = removing_request.into_writes();
    let system = [
        VersionlessEntry::put(b"session".to_vec(), b"entry".to_vec()),
        VersionlessEntry::removal(b"evicted".to_vec()),
    ];
    lists.store().apply(
        single_versionless(b"evicted"),
        ultragraphdb_traits_store::PersistMode::Buffer,
    )?;
    lists.commit_round_with(&[(4, &removing)], &system, 6)?;
    assert_eq!(
        lists.store().get_versionless(b"gone")?,
        None,
        "the removing_request's removal"
    );
    assert_eq!(
        lists.store().get_versionless(b"session")?,
        Some(b"entry".to_vec())
    );
    assert_eq!(
        lists.store().get_versionless(b"evicted")?,
        None,
        "the system's removal"
    );
    assert_eq!((lists.applied_ts(), lists.store().applied_ts()), (6, 6));
    assert_rounds_of_the_systems_entries_alone(&lists, &system)?;
    Ok(())
}

/// Rounds of the system's entries alone over `lists`, its watermark at 6 and `system` written: above, at and below
/// the watermark, and a round with nothing to write.
fn assert_rounds_of_the_systems_entries_alone(
    lists: &TestLists,
    system: &[VersionlessEntry],
) -> TestResult {
    // A round of the system's entries alone.
    lists.commit_round_with(
        &[],
        &[VersionlessEntry::put(b"alone".to_vec(), b"x".to_vec())],
        8,
    )?;
    assert_eq!(
        lists.store().get_versionless(b"alone")?,
        Some(b"x".to_vec())
    );
    assert_eq!(lists.applied_ts(), 8);
    // At the watermark, as after a drop that moved it: the entries are written and the watermark stays.
    lists.commit_round_with(
        &[],
        &[VersionlessEntry::put(b"after".to_vec(), b"y".to_vec())],
        8,
    )?;
    assert_eq!(
        lists.store().get_versionless(b"after")?,
        Some(b"y".to_vec())
    );
    assert_eq!((lists.applied_ts(), lists.store().applied_ts()), (8, 8));
    // Below the watermark it is refused naming both, and nothing is written.
    assert_eq!(
        lists.commit_round_with(&[], system, 6),
        Err(PostingError::stale_version(6, 8))
    );
    assert_eq!(
        lists.store().get_versionless(b"session")?,
        Some(b"entry".to_vec())
    );
    // Nothing to write and a version above the watermark: the watermark moves.
    lists.commit_round_with(&[], &[], 10)?;
    assert_eq!((lists.applied_ts(), lists.store().applied_ts()), (10, 10));
    Ok(())
}

/// A batch holding one entry without a version, `key`.
fn single_versionless(key: &[u8]) -> ultragraphdb_traits_store::WriteBatch {
    let mut batch = ultragraphdb_traits_store::WriteBatch::new();
    batch.put_versionless(key.to_vec(), b"present".to_vec());
    batch
}

/// A version at or below the watermark, or not above the round's previous one, is refused naming both, and nothing of
/// the round is written.
#[test]
fn test_a_round_with_a_stale_version_is_refused_whole() -> TestResult {
    let lists = new_lists();
    let ts = crate::utils::helpers::commit_edges_at(&lists, &key("stale", 1), &[set_uid(1)], 6)?;
    let mut request = lists.request(ts);
    request
        .get(&key("stale", 2))?
        .add_posting(set_uid(2), false);
    let writes = request.into_writes();
    let before = lists.store().contents();
    for (round, refused) in [
        (vec![(6, &writes)], PostingError::stale_version(6, 6)),
        (vec![(4, &writes)], PostingError::stale_version(4, 6)),
        (
            vec![(8, &writes), (8, &writes)],
            PostingError::stale_version(8, 8),
        ),
        (
            vec![(10, &writes), (8, &writes)],
            PostingError::stale_version(8, 10),
        ),
    ] {
        assert_eq!(lists.commit_round(&round), Err(refused));
        assert_eq!(lists.store().contents(), before);
        assert_eq!(lists.applied_ts(), 6);
    }
    lists.commit_round(&[(7, &writes)])?;
    assert_eq!(
        lists.applied_ts(),
        7,
        "any version above the watermark is taken"
    );
    Ok(())
}

/// A round that fails to write stops every later write and refuses the reads at or above its first version; reads at
/// the watermark go ahead.
#[test]
fn test_a_failed_round_stops_writes_and_the_reads_above_it() -> TestResult {
    let lists = new_lists();
    let ok =
        crate::utils::helpers::commit_edges_at(&lists, &key("round-fail", 1), &[set_uid(1)], 4)?;
    lists.store().inject(Fault::FailNextBatch);
    let mut request = lists.request(ok);
    request
        .get_from_delta(&key("round-fail", 2))
        .add_posting(set_uid(2), false);
    let writes = request.into_writes();
    assert!(matches!(
        lists
            .commit_round(&[(8, &writes)])
            .failure("a round at a stale version")?
            .0,
        PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
    ));
    let poisoned = PostingError::store(StoreError::poisoned());
    assert_eq!(lists.applied_ts(), ok);
    assert_eq!(lists.try_gate(8), Err(poisoned.clone()));
    assert!(lists.try_gate(ok)?);
    assert_eq!(lists.commit_round(&[(10, &writes)]), Err(poisoned));
    Ok(())
}

#[test]
fn test_a_read_inside_a_commits_check_neither_deadlocks_nor_fills() -> TestResult {
    let lists = new_lists();
    let k = key("inside", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)])?;
    lists.cache().remove(&k);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request
        .get(&key("inside", 2))?
        .add_posting(set_uid(2).clone(), false);
    batch.push(request);
    let next = lists.next_version();
    let results = lists.commit_with(batch, |_| {
        let read = lists.read(&k, ts)?;
        assert_eq!(uids(&read, ts, 0)?, vec![1]);
        Ok::<_, TestError>(())
    })?;
    assert_eq!(results, vec![Ok(next)]);
    assert!(
        lists.cache().get(&k).is_none(),
        "the fill found the writer lock held"
    );
    Ok(())
}

#[test]
fn test_a_failed_commit_is_reported_as_an_error() -> TestResult {
    let capture = ultragraphdb_store_sim::LogCapture::new();
    capture.capture(|| -> TestResult {
        let lists = new_lists();
        lists.store().inject(Fault::FailNextBatch);
        let mut batch = lists.batch();
        let mut request = batch.request(lists.applied_ts());
        request
            .get(&key("reported", 1))?
            .add_posting(set_uid(1).clone(), false);
        batch.push(request);
        assert!(matches!(
            lists.commit(batch).failure("the commit")?.0,
            PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
        ));
        Ok(())
    })?;
    let events = capture.events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].level(), tracing::Level::ERROR);
    assert!(events[0].field("first_commit_ts").is_some());
    assert!(events[0].field("error").is_some());
    Ok(())
}

/// An entry added with its version kept is written at that version, the request's other writes at its commit
/// timestamp: what a rebuild's swap writes, its built lists at the rebuild's timestamp.
#[test]
fn test_an_entry_at_its_own_version_keeps_it() -> TestResult {
    let lists = new_lists();
    let (a, b) = (key("versioned", 1), key("versioned", 2));
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request.add_entry_at_version(Entry::deletion(a.clone(), 1));
    request.add_entry(Entry::deletion(b.clone(), 1));
    batch.push(request);
    let ts = lists.commit(batch)?[0];
    let contents = lists.store().contents();
    assert_eq!(contents.versions(&a)[0].version(), 1);
    assert_eq!(contents.versions(&b)[0].version(), ts);
    Ok(())
}

/// A request marked to advance the epoch does so under the writer lock with its batch, so no cached list read
/// before the batch serves a read after it.
#[test]
fn test_a_commit_that_advances_the_epoch_leaves_no_older_entry_serving() -> TestResult {
    let lists = new_lists();
    let k = key("epoch", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)])?;
    let _k_list: ultragraphdb_store_posting::List = lists.read(&k, ts)?;
    assert!(lists.cached(&k, ts)?.is_some());
    let epoch = lists.epoch();
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request.add_entry_at_version(Entry::deletion(k.clone(), ts));
    request.advance_epoch_on_commit();
    batch.push(request);
    let swapped = lists.commit(batch)?[0];
    assert_eq!(lists.epoch(), epoch + 1);
    assert!(lists.cached(&k, swapped)?.is_none());
    assert!(uids(&lists.read(&k, swapped)?, swapped, 0)?.is_empty());
    Ok(())
}

/// A request marked to remove a rebuild record removes it in its commit batch, atomically with its writes; the
/// record of a request left out stays.
#[test]
fn test_a_commit_removes_the_rebuild_records_its_requests_name() -> TestResult {
    use ultragraphdb_traits_store::RebuildRecord;
    let lists = new_lists();
    let store = lists.store();
    store.write_rebuild_record(&RebuildRecord::new(4, vec![vec![0x00]]))?;
    store.write_rebuild_record(&RebuildRecord::new(5, vec![vec![0x00]]))?;
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
    let results = lists.commit_with(batch, |_| {
        seen += 1;
        if seen == 2 { Err("refused") } else { Ok(()) }
    })?;
    assert_eq!(results, vec![Ok(next), Err("refused")]);
    let records: Vec<u64> = store.rebuild_records()?.iter().map(|r| r.id()).collect();
    assert_eq!(records, [5]);
    Ok(())
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
) -> TestResult {
    let mut request = batch.request(lists.applied_ts());
    request.get(k)?.add_posting(set_uid(1), false);
    if advance {
        request.advance_epoch_on_commit();
    }
    batch.push(request);
    Ok(())
}

#[test]
fn test_the_requests_kept_take_successive_versions_around_one_refused() -> TestResult {
    let lists = new_lists();
    let keys: Vec<_> = (1..=3).map(|i| key("around", i)).collect();
    let mut batch = lists.batch();
    keys.iter()
        .try_for_each(|k| push_write(&mut batch, &lists, k, false))?;
    let next = lists.next_version();
    let mut seen = 0;
    let results = lists.commit_with(batch, |_| {
        seen += 1;
        if seen == 2 { Err("refused") } else { Ok(()) }
    })?;
    assert_eq!(results, vec![Ok(next), Err("refused"), Ok(next + 2)]);
    assert_eq!(
        (lists.applied_ts(), lists.next_version()),
        (next + 2, next + 4)
    );
    let contents = lists.store().contents();
    assert_eq!(contents.versions(&keys[2])[0].version(), next + 2);
    assert!(contents.versions(&keys[1]).is_empty());
    Ok(())
}

#[test]
fn test_a_refused_request_does_not_advance_the_epoch() -> TestResult {
    let lists = new_lists();
    let epoch = lists.epoch();
    let mut batch = lists.batch();
    push_write(&mut batch, &lists, &key("refused-epoch", 1), true)?;
    push_write(&mut batch, &lists, &key("refused-epoch", 2), false)?;
    let mut seen = 0;
    let _results: Vec<Result<u64, ()>> = lists.commit_with(batch, |_| {
        seen += 1;
        if seen == 1 { Err(()) } else { Ok(()) }
    })?;
    assert_eq!(lists.epoch(), epoch);
    Ok(())
}

#[test]
fn test_a_commit_removes_a_cached_list_of_an_older_epoch() -> TestResult {
    let lists = new_lists();
    let k = key("older-epoch", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)])?;
    let _k_list: ultragraphdb_store_posting::List = lists.read(&k, ts)?;
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
    let _versions: Vec<u64> = lists.commit(batch)?;
    assert!(lists.cache().get(&k).is_none());
    Ok(())
}

#[test]
fn test_a_commit_leaves_an_uncached_list_uncached() -> TestResult {
    let lists = new_lists();
    let k = key("uncached", 1);
    commit(&lists, &k, &[set_uid(1)])?;
    lists.cache().remove(&k);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request.get_from_delta(&k).add_posting(set_uid(2), false);
    batch.push(request);
    let _versions: Vec<u64> = lists.commit(batch)?;
    assert!(lists.cache().get(&k).is_none());
    Ok(())
}

#[test]
fn test_an_empty_batch_commits_nothing() -> TestResult {
    let lists = new_lists();
    let next = lists.next_version();
    assert_eq!(lists.commit(lists.batch())?, Vec::<u64>::new());
    assert_eq!((lists.next_version(), lists.applied_ts()), (next, 0));
    Ok(())
}

#[test]
fn test_a_commit_queues_each_written_key_at_both_priorities_and_no_read_key() -> TestResult {
    let lists = new_lists();
    let (written, read) = (key("queued", 1), key("queued", 2));
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    let _read_list: &mut ultragraphdb_store_posting::List = request.get(&read)?;
    request.get(&written)?.add_posting(set_uid(1), false);
    batch.push(request);
    let _versions: Vec<u64> = lists.commit(batch)?;
    assert_eq!(lists.rollups_waiting(), 2);
    Ok(())
}

/// The lists refuse every write after a failed one on their own, even where the store would take it: a recovered
/// store is reopened with new lists, never written through the old ones.
#[test]
fn test_lists_refuse_writes_after_a_failure_whatever_the_store_says() -> TestResult {
    let lists = new_lists();
    let k = key("refuse", 1);
    let ok = commit_edges(&lists, &k, &[set_uid(1)])?;
    lists.store().inject(Fault::FailNextBatch);
    let mut batch = lists.batch();
    let mut request = batch.request(ok);
    request.get_from_delta(&k).add_posting(set_uid(2), false);
    batch.push(request);
    assert!(lists.commit(batch).is_err());
    lists.store().recover();
    let before = lists.store().contents();
    let poisoned = PostingError::store(StoreError::poisoned());
    let mut refused_batch = lists.batch();
    let mut refused_request = refused_batch.request(ok);
    refused_request
        .get_from_delta(&k)
        .add_posting(set_uid(3), false);
    refused_batch.push(refused_request);
    assert_eq!(lists.commit(refused_batch), Err(poisoned.clone()));
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
    Ok(())
}
