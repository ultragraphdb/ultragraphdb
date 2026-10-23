/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Garbage collection below the discard timestamp, as the kv-store specification states it.

use ultragraphdb_store_sim::{Corruption, FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

fn store() -> SimStore {
    SimStore::new(Seed::new(2), FaultConfig::none(), SimClock::new())
}

fn write(store: &SimStore, entry: Entry) {
    let mut batch = WriteBatch::new();
    batch.push(entry);
    store
        .apply(batch, PersistMode::Buffer)
        .expect("no fault is configured");
}

fn held(store: &SimStore, key: &[u8]) -> Vec<(u64, bool)> {
    store
        .contents()
        .versions(key)
        .iter()
        .map(|v| (v.version(), v.is_deletion()))
        .collect()
}

#[test]
fn test_versions_below_a_discard_point_are_collected() {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"d2".to_vec()));
    write(&s, Entry::put(b"k".to_vec(), 4, 0x04, b"d4".to_vec()));
    write(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c5".to_vec()).with_discard(),
    );
    write(&s, Entry::put(b"k".to_vec(), 8, 0x04, b"d8".to_vec()));
    s.set_discard_ts(6).unwrap();
    s.compact().unwrap();
    assert_eq!(held(&s, b"k"), vec![(8, false), (5, false)]);
}

#[test]
fn test_a_deletion_is_never_collected_and_hides_what_is_below_it() {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    write(&s, Entry::deletion(b"k".to_vec(), 4));
    s.set_discard_ts(6).unwrap();
    s.compact().unwrap();
    assert_eq!(held(&s, b"k"), vec![(4, true)]);
    assert_eq!(s.get_at(b"k", 7).unwrap(), None);
}

#[test]
fn test_the_sweep_removes_a_deletion_with_nothing_below_it() {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    write(&s, Entry::deletion(b"k".to_vec(), 4));
    s.set_discard_ts(6).unwrap();
    assert_eq!(
        s.tombstone_horizon(),
        6,
        "the horizon follows the discard timestamp until it is set"
    );
    s.compact().unwrap();
    s.sweep_deletions().unwrap();
    assert!(held(&s, b"k").is_empty());
}

#[test]
fn test_a_deletion_above_the_tombstone_horizon_is_kept() {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    write(&s, Entry::deletion(b"k".to_vec(), 5));
    s.set_discard_ts(6).unwrap();
    s.set_tombstone_horizon(4).unwrap();
    s.compact().unwrap();
    s.sweep_deletions().unwrap();
    assert_eq!(held(&s, b"k"), vec![(5, true)]);
}

#[test]
fn test_the_sweep_keeps_a_deletion_with_a_version_below_it() {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    write(&s, Entry::deletion(b"k".to_vec(), 4));
    s.set_discard_ts(6).unwrap();
    s.sweep_deletions().unwrap();
    assert_eq!(held(&s, b"k"), vec![(4, true), (2, false)]);
}

#[test]
fn test_an_undecodable_item_survives_compaction() {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    write(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
    );
    s.corrupt(b"k", 2, Corruption::Truncate);
    s.set_discard_ts(6).unwrap();
    s.compact().unwrap();
    assert!(!s.needs_recovery(), "the store is not poisoned");
    let versions = s.contents().versions(b"k").to_vec();
    assert_eq!(versions.len(), 2);
    assert!(!versions[1].is_decodable());
}

#[test]
fn test_an_open_reader_holds_the_discard_timestamp_back() {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    write(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
    );
    let reader = s.snapshot(3).unwrap();
    s.set_discard_ts(6).unwrap();
    assert_eq!(s.discard_ts(), 3);
    assert_eq!(s.tombstone_horizon(), 3);
    s.compact().unwrap();
    assert_eq!(
        s.contents().versions(b"k").len(),
        2,
        "the version the reader sees survives"
    );
    assert!(
        s.snapshot(3).is_ok(),
        "another reader at the held timestamp opens"
    );
    drop(reader);
    assert_eq!(s.discard_ts(), 6);
    assert!(s.snapshot(3).is_err());
}

#[test]
fn test_the_discard_timestamp_never_decreases() {
    let s = store();
    s.set_discard_ts(6).unwrap();
    s.set_discard_ts(4).unwrap();
    assert_eq!(s.discard_ts(), 6);
}

#[test]
fn test_the_initial_schema_a_drop_writes_survives_garbage_collection() {
    // A prefix drop fences the entries it writes: compaction never collects them, as on fjall, where a version the
    // drop removed can reach a compaction beneath a removal its filter is not shown. A drop of everything clears and
    // fences nothing.
    let s = store();
    let schema = b"\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x04name".to_vec();
    write(
        &s,
        Entry::put(schema.clone(), 5, 0x01, b"old".to_vec()).with_discard(),
    );
    s.drop_prefixes(
        7,
        &[schema[..9].to_vec()],
        vec![Entry::put(schema.clone(), 1, 0x01, b"initial".to_vec())],
    )
    .unwrap();
    write(
        &s,
        Entry::put(schema.clone(), 8, 0x01, b"new".to_vec()).with_discard(),
    );
    s.set_discard_ts(9).unwrap();
    s.compact().unwrap();
    assert_eq!(held(&s, &schema), vec![(8, false), (1, false)]);
    s.drop_all(vec![Entry::put(schema.clone(), 1, 0x01, b"root".to_vec())])
        .unwrap();
    write(
        &s,
        Entry::put(schema.clone(), 3, 0x01, b"new".to_vec()).with_discard(),
    );
    s.set_discard_ts(4).unwrap();
    s.compact().unwrap();
    assert_eq!(held(&s, &schema), vec![(3, false)]);
}
