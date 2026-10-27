/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_posting::{BIT_SCHEMA_POSTING, PostingError, PostingErrorEnum};
use ultragraphdb_store_sim::{DropStage, Fault};
use ultragraphdb_traits_store::{Entry, Store};

use crate::utils::helpers::{commit_edges, key, new_lists, set_uid, uids};

#[test]
fn test_a_drop_advances_the_epoch_and_clears_the_cache() {
    let lists = new_lists();
    let k = key("dropped", 1);
    let ts = commit_edges(&lists, &k, &[set_uid(1)]);
    lists.read(&k, ts).unwrap();
    let epoch = lists.epoch();
    lists
        .drop_prefixes(lists.applied_ts(), std::slice::from_ref(&k), Vec::new())
        .unwrap();
    assert_eq!(lists.epoch(), epoch + 1);
    assert!(lists.cached(&k, ts).unwrap().is_none());
    let after = lists.applied_ts();
    assert!(uids(&lists.read(&k, after).unwrap(), after, 0).is_empty());
}

#[test]
fn test_a_drop_of_everything_keeps_its_entries() {
    let lists = new_lists();
    let ts = commit_edges(&lists, &key("all", 1), &[set_uid(1)]);
    lists
        .drop_all(vec![Entry::put(
            b"\x01s".to_vec(),
            1,
            BIT_SCHEMA_POSTING,
            b"x".to_vec(),
        )])
        .unwrap();
    let contents = lists.store().contents();
    assert_eq!(contents.keys().len(), 1);
    assert!(lists.cached(&key("all", 1), ts).unwrap().is_none());
}

#[test]
fn test_the_discard_timestamp_stays_at_or_below_the_oldest_open_request() {
    let lists = new_lists();
    let ts = commit_edges(&lists, &key("discard", 1), &[set_uid(1)]);
    let request = lists.request(ts);
    assert_eq!(lists.oldest_open_read(), Some(ts));
    lists.set_discard_ts(ts + 100).unwrap();
    assert!(lists.store().discard_ts() <= ts);
    drop(request);
    assert_eq!(lists.oldest_open_read(), None);
    let later = lists.applied_ts();
    lists.set_discard_ts(later).unwrap();
    assert_eq!(lists.store().discard_ts(), later);
}

/// A drop that takes the next version under the writer holds every read above the watermark until it is done, as a
/// round being written holds them, so no read at the drop's version sees the data it removes; the watermark then moves
/// to the drop's version.
#[test]
fn test_a_drop_at_the_next_version_holds_reads_above_the_watermark() {
    let (lists, probe) = crate::utils::helpers::probe_lists();
    let k = key("held", 1);
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    request
        .get(&k)
        .unwrap()
        .add_posting(set_uid(1).clone(), false);
    batch.push(request);
    let watermark = lists.commit(batch).unwrap()[0];
    let expected = lists.next_version();
    assert_eq!(expected, watermark + 2);
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (inner, record) = (std::sync::Arc::clone(&lists), std::sync::Arc::clone(&seen));
    probe.on_next_drop(Box::new(move || {
        let mut seen = record.lock().unwrap();
        seen.push(inner.try_gate(watermark).unwrap());
        seen.push(inner.try_gate(expected).unwrap());
    }));
    let ts = lists
        .drop_prefixes_now(std::slice::from_ref(&k), Vec::new())
        .unwrap();
    assert_eq!(ts, expected);
    assert_eq!(*seen.lock().unwrap(), vec![true, false]);
    assert_eq!(lists.applied_ts(), ts);
    assert!(lists.try_gate(ts).unwrap());
    assert!(uids(&lists.read(&k, ts).unwrap(), ts, 0).is_empty());
}

#[test]
fn test_a_drop_of_everything_at_the_next_version_holds_reads_above_the_watermark() {
    let (lists, probe) = crate::utils::helpers::probe_lists();
    let expected = lists.next_version();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let (inner, record) = (std::sync::Arc::clone(&lists), std::sync::Arc::clone(&seen));
    probe.on_next_drop(Box::new(move || {
        *record.lock().unwrap() = Some(inner.try_gate(expected).unwrap());
    }));
    let epoch = lists.epoch();
    let ts = lists.drop_all_now(Vec::new()).unwrap();
    assert_eq!(ts, expected);
    assert_eq!(*seen.lock().unwrap(), Some(false));
    assert_eq!(lists.epoch(), epoch + 1);
    assert!(lists.try_gate(ts).unwrap());
    assert_eq!(lists.applied_ts(), ts);
}

fn write_failed(error: PostingError) -> bool {
    matches!(
        error.0,
        PostingErrorEnum::Store(StoreError(StoreErrorEnum::WriteFailed(_)))
    )
}

/// A drop that fails still advances the epoch and stops every later write; a read at the watermark goes ahead, and one
/// above it is refused, as nothing will take the watermark there.
#[test]
fn test_a_failed_drop_advances_the_epoch_and_stops_writes() {
    type Drop = fn(&crate::utils::helpers::TestLists, &[u8]) -> Result<(), PostingError>;
    let drops: [(&str, Drop); 4] = [
        ("drop_prefixes", |l, k| {
            l.drop_prefixes(l.applied_ts(), &[k.to_vec()], Vec::new())
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
        let ts = commit_edges(&lists, &k, &[set_uid(1)]);
        let epoch = lists.epoch();
        lists
            .store()
            .inject(Fault::CrashDuringNextDrop(DropStage::Start));
        assert!(write_failed(drop(&lists, &k).unwrap_err()), "{name}");
        assert_eq!(lists.epoch(), epoch + 1, "{name}");
        let poisoned = Err(PostingError::Store(StoreError::Poisoned()));
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
        assert!(lists.try_gate(ts).unwrap(), "{name}");
    }
}

#[test]
fn test_a_drop_at_a_given_timestamp_moves_no_version() {
    let lists = new_lists();
    let k = key("drop-given", 1);
    commit_edges(&lists, &k, &[set_uid(1)]);
    let next = lists.next_version();
    lists
        .drop_prefixes(next + 10, std::slice::from_ref(&k), Vec::new())
        .unwrap();
    assert_eq!(lists.next_version(), next);
    lists.drop_all(Vec::new()).unwrap();
    assert_eq!(lists.next_version(), next);
    assert_eq!(lists.drop_all_now(Vec::new()).unwrap(), next);
    assert_eq!(lists.drop_prefixes_now(&[k], Vec::new()).unwrap(), next + 2);
    assert_eq!(lists.next_version(), next + 4);
}
