/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::PoisonError;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::{BIT_SCHEMA_POSTING, PostingError, PostingErrorEnum};
use ultragraphdb_store_sim::{DropStage, Fault};
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_store::{Entry, Store};

use crate::utils::helpers::{Failure, commit, commit_edges, key, new_lists, set_uid, uids};

#[test]
fn test_a_drop_advances_the_epoch_and_clears_the_cache() -> TestResult {
    let lists = new_lists();
    let k = key("dropped", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)])?;
    let _k_list: ultragraphdb_store_posting::List = lists.read(&k, ts)?;
    let epoch = lists.epoch();
    lists.drop_prefixes(lists.applied_ts(), std::slice::from_ref(&k), Vec::new())?;
    assert_eq!(lists.epoch(), epoch + 1);
    assert!(lists.cached(&k, ts)?.is_none());
    let after = lists.applied_ts();
    assert!(uids(&lists.read(&k, after)?, after, 0)?.is_empty());
    Ok(())
}

#[test]
fn test_a_drop_of_everything_keeps_its_entries() -> TestResult {
    let lists = new_lists();
    let ts = commit_edges(&lists, &key("all", 1), &[set_uid(1)])?;
    lists.drop_all(vec![Entry::put(
        b"\x01s".to_vec(),
        1,
        BIT_SCHEMA_POSTING,
        b"x".to_vec(),
    )])?;
    let contents = lists.store().contents();
    assert_eq!(contents.keys().len(), 1);
    assert!(lists.cached(&key("all", 1), ts)?.is_none());
    Ok(())
}

#[test]
fn test_the_discard_timestamp_stays_at_or_below_the_oldest_open_request() -> TestResult {
    let lists = new_lists();
    let ts = commit_edges(&lists, &key("discard", 1), &[set_uid(1)])?;
    let request = lists.request(ts);
    assert_eq!(lists.oldest_open_read(), Some(ts));
    lists.set_discard_ts(ts + 100)?;
    assert!(lists.store().discard_ts() <= ts);
    drop(request);
    assert_eq!(lists.oldest_open_read(), None);
    let later = lists.applied_ts();
    lists.set_discard_ts(later)?;
    assert_eq!(lists.store().discard_ts(), later);
    Ok(())
}

/// A drop that takes the next version under the writer holds every read above the watermark until it is done, as a
/// round being written holds them, so no read at the drop's version sees the data it removes; the watermark then moves
/// to the drop's version.
#[test]
fn test_a_drop_at_the_next_version_holds_reads_above_the_watermark() -> TestResult {
    let (lists, probe) = crate::utils::helpers::probe_lists();
    let k = key("held", 1);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request.get(&k)?.add_posting(set_uid(1).clone(), false);
    batch.push(request);
    let watermark = lists.commit(batch)?[0];
    let expected = lists.next_version();
    assert_eq!(expected, watermark + 2);
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (inner, record) = (std::sync::Arc::clone(&lists), std::sync::Arc::clone(&seen));
    probe.on_next_drop(Box::new(move || -> TestResult {
        let mut gates = record.lock().unwrap_or_else(PoisonError::into_inner);
        gates.push(inner.try_gate(watermark)?);
        gates.push(inner.try_gate(expected)?);
        Ok(())
    }));
    let ts = lists.drop_prefixes_now(std::slice::from_ref(&k), Vec::new())?;
    assert_eq!(ts, expected);
    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        vec![true, false]
    );
    assert_eq!(lists.applied_ts(), ts);
    assert!(lists.try_gate(ts)?);
    assert!(uids(&lists.read(&k, ts)?, ts, 0)?.is_empty());
    Ok(())
}

#[test]
fn test_a_drop_of_everything_at_the_next_version_holds_reads_above_the_watermark() -> TestResult {
    let (lists, probe) = crate::utils::helpers::probe_lists();
    let expected = lists.next_version();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let (inner, record) = (std::sync::Arc::clone(&lists), std::sync::Arc::clone(&seen));
    probe.on_next_drop(Box::new(move || -> TestResult {
        *record.lock().unwrap_or_else(PoisonError::into_inner) = Some(inner.try_gate(expected)?);
        Ok(())
    }));
    let epoch = lists.epoch();
    let ts = lists.drop_all_now(Vec::new())?;
    assert_eq!(ts, expected);
    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        Some(false)
    );
    assert_eq!(lists.epoch(), epoch + 1);
    assert!(lists.try_gate(ts)?);
    assert_eq!(lists.applied_ts(), ts);
    Ok(())
}

fn write_failed(error: &PostingError) -> bool {
    matches!(
        error.0,
        PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
    )
}

/// A drop that fails still advances the epoch and stops every later write; a read at the watermark goes ahead, and one
/// above it is refused, as nothing will take the watermark there.
#[test]
fn test_a_failed_drop_advances_the_epoch_and_stops_writes() -> TestResult {
    type Drop = fn(&crate::utils::helpers::TestLists, &[u8]) -> Result<(), PostingError>;
    let drops: [(&str, Drop); 5] = [
        ("drop_prefixes", |l, k| {
            l.drop_prefixes(l.applied_ts(), &[k.to_vec()], Vec::new())
        }),
        ("drop_prefixes_at", |l, k| {
            l.drop_prefixes_at(l.applied_ts() + 2, &[k.to_vec()], Vec::new())
        }),
        ("drop_prefixes_now", |l, k| {
            l.drop_prefixes_now(&[k.to_vec()], Vec::new()).map(|_| ())
        }),
        ("drop_all", |l, _| l.drop_all(Vec::new())),
        ("drop_all_now", |l, _| {
            l.drop_all_now(Vec::new()).map(|_| ())
        }),
    ];
    for (name, drop) in drops {
        let lists = new_lists();
        let k = key("failed-drop", 1);
        let ts = commit_edges(&lists, &k, &[set_uid(1)])?;
        let epoch = lists.epoch();
        lists
            .store()
            .inject(Fault::CrashDuringNextDrop(DropStage::Start));
        assert!(
            write_failed(&drop(&lists, &k).failure("the drop through a crash")?),
            "{name}"
        );
        assert_eq!(lists.epoch(), epoch + 1, "{name}");
        let poisoned = Err(PostingError::store(StoreError::poisoned()));
        assert_eq!(drop(&lists, &k), poisoned, "{name}: a later drop");
        assert_eq!(
            lists.drop_all(Vec::new()),
            poisoned,
            "{name}: a later drop of everything"
        );
        let next = lists.next_version();
        assert_eq!(
            lists.try_gate(next),
            poisoned.map(|()| false),
            "{name}: no read waits on it"
        );
        assert!(lists.try_gate(ts)?, "{name}");
    }
    Ok(())
}

#[test]
fn test_a_drop_at_a_given_timestamp_moves_no_version() -> TestResult {
    let lists = new_lists();
    let k = key("drop-given", 1);
    commit(&lists, &k, &[set_uid(1)])?;
    let next = lists.next_version();
    lists.drop_prefixes(next + 10, std::slice::from_ref(&k), Vec::new())?;
    assert_eq!(lists.next_version(), next);
    lists.drop_all(Vec::new())?;
    assert_eq!(lists.next_version(), next);
    assert_eq!(lists.drop_all_now(Vec::new())?, next);
    assert_eq!(lists.drop_prefixes_now(&[k], Vec::new())?, next + 2);
    assert_eq!(lists.next_version(), next + 4);
    Ok(())
}

/// A drop at a version the commit step gave it drops every version of its keys and moves the watermark to that
/// version; one at or below the applied version is refused, writing nothing (7.7).
#[test]
fn test_a_drop_at_a_given_version_drops_its_keys_and_moves_the_watermark_there() -> TestResult {
    let lists = new_lists();
    let k = key("dropped", 1);
    let kept = key("kept", 1);
    commit(&lists, &k, &[set_uid(2)])?;
    commit(&lists, &kept, &[set_uid(3)])?;
    let applied = lists.applied_ts();
    let stale = lists
        .drop_prefixes_at(applied, std::slice::from_ref(&k), Vec::new())
        .failure("a drop at the watermark")?;
    assert_eq!(
        stale.0,
        PostingErrorEnum::StaleVersion {
            version: applied,
            after: applied
        }
    );
    assert_eq!(lists.applied_ts(), applied);
    lists.drop_prefixes_at(applied + 10, std::slice::from_ref(&k), Vec::new())?;
    assert_eq!(lists.applied_ts(), applied + 10);
    let read = lists.applied_ts();
    assert!(uids(&lists.read(&k, read)?, read, 0)?.is_empty());
    assert_eq!(uids(&lists.read(&kept, read)?, read, 0)?, vec![3]);
    Ok(())
}

/// A drop at the watermark and a record's removal take no version: the watermark stays, the keys are gone, the
/// record removed (decided 2026-10-28).
#[test]
fn test_cleanups_at_the_watermark_take_no_version() -> TestResult {
    use ultragraphdb_traits_store::{RebuildRecord, Store};
    let lists = new_lists();
    let ts = crate::utils::helpers::commit_edges_at(&lists, &key("gone", 1), &[set_uid(1)], 6)?;
    lists
        .store()
        .write_rebuild_record(&RebuildRecord::new(4, vec![b"p".to_vec()]))?;
    let prefix = key("gone", 1);
    lists.drop_prefixes_at_watermark(std::slice::from_ref(&prefix))?;
    lists.remove_rebuild_record(4)?;
    assert_eq!((lists.applied_ts(), lists.store().applied_ts()), (ts, ts));
    assert!(lists.store().get_at(&prefix, ts)?.is_none());
    assert!(lists.store().rebuild_records()?.is_empty());
    Ok(())
}
