/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Garbage collection below the discard timestamp, as the kv-store specification states it.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_sim::{Corruption, FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, StoreSnapshot, WriteBatch};

fn store() -> SimStore {
    SimStore::new(Seed::new(2), FaultConfig::none(), SimClock::new())
}

fn write(store: &SimStore, entry: Entry) -> TestResult {
    let mut batch = WriteBatch::new();
    batch.push(entry);
    store
        .apply(batch, PersistMode::Buffer)
        .context("no fault is configured")?;
    Ok(())
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
fn test_versions_below_a_discard_point_are_collected() -> TestResult {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"d2".to_vec()))?;
    write(&s, Entry::put(b"k".to_vec(), 4, 0x04, b"d4".to_vec()))?;
    write(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c5".to_vec()).with_discard(),
    )?;
    write(&s, Entry::put(b"k".to_vec(), 8, 0x04, b"d8".to_vec()))?;
    s.set_discard_ts(6)?;
    s.compact()?;
    assert_eq!(held(&s, b"k"), vec![(8, false), (5, false)]);
    Ok(())
}

#[test]
fn test_a_discard_point_counts_at_the_discard_timestamp_and_not_above_it() -> TestResult {
    // The cut is the newest discard-flagged version at or below D: one at D cuts, one at D + 1 does not.
    for (discard, expected) in [
        (4, vec![(5, false), (4, false), (2, false)]),
        (5, vec![(5, false)]),
        (6, vec![(5, false)]),
    ] {
        let s = store();
        write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"d2".to_vec()))?;
        write(&s, Entry::put(b"k".to_vec(), 4, 0x04, b"d4".to_vec()))?;
        write(
            &s,
            Entry::put(b"k".to_vec(), 5, 0x08, b"c5".to_vec()).with_discard(),
        )?;
        s.set_discard_ts(discard)?;
        s.compact()?;
        assert_eq!(held(&s, b"k"), expected, "discard {discard}");
    }
    Ok(())
}

#[test]
fn test_the_newest_discard_point_below_the_timestamp_is_the_cut() -> TestResult {
    let s = store();
    for (v, discard) in [(1, false), (2, true), (3, false), (4, true), (6, true)] {
        let entry = Entry::put(b"k".to_vec(), v, 0x04, vec![]);
        write(&s, if discard { entry.with_discard() } else { entry })?;
    }
    s.set_discard_ts(5)?;
    s.compact()?;
    assert_eq!(held(&s, b"k"), vec![(6, false), (4, false)]);
    Ok(())
}

#[test]
fn test_compaction_leaves_keys_without_a_discard_point_whole() -> TestResult {
    let s = store();
    write(&s, Entry::put(b"a".to_vec(), 1, 0x04, vec![]))?;
    write(&s, Entry::put(b"a".to_vec(), 2, 0x04, vec![]))?;
    write(&s, Entry::put(b"b".to_vec(), 1, 0x04, vec![]))?;
    write(
        &s,
        Entry::put(b"b".to_vec(), 3, 0x04, vec![]).with_discard(),
    )?;
    s.set_discard_ts(9)?;
    s.compact()?;
    assert_eq!(held(&s, b"a"), vec![(2, false), (1, false)]);
    assert_eq!(held(&s, b"b"), vec![(3, false)]);
    Ok(())
}

#[test]
fn test_a_deletion_is_never_collected_and_hides_what_is_below_it() -> TestResult {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    write(&s, Entry::deletion(b"k".to_vec(), 4))?;
    s.set_discard_ts(6)?;
    s.compact()?;
    assert_eq!(held(&s, b"k"), vec![(4, true)]);
    assert_eq!(s.get_at(b"k", 7)?, None);
    Ok(())
}

#[test]
fn test_the_sweep_removes_a_deletion_with_nothing_below_it() -> TestResult {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    write(&s, Entry::deletion(b"k".to_vec(), 4))?;
    s.set_discard_ts(6)?;
    assert_eq!(
        s.tombstone_horizon(),
        6,
        "the horizon follows the discard timestamp until it is set"
    );
    s.compact()?;
    s.sweep_deletions()?;
    assert!(held(&s, b"k").is_empty());
    assert!(s.contents().keys().is_empty());
    Ok(())
}

#[test]
fn test_the_sweep_takes_a_deletion_at_the_horizon_and_not_above_it() -> TestResult {
    for (horizon, expected) in [(3, vec![(4, true)]), (4, vec![]), (5, vec![])] {
        let s = store();
        write(&s, Entry::deletion(b"k".to_vec(), 4))?;
        s.set_discard_ts(9)?;
        s.set_tombstone_horizon(horizon)?;
        s.sweep_deletions()?;
        assert_eq!(held(&s, b"k"), expected, "horizon {horizon}");
    }
    Ok(())
}

#[test]
fn test_the_tombstone_horizon_is_held_at_the_discard_timestamp() -> TestResult {
    let s = store();
    s.set_discard_ts(6)?;
    s.set_tombstone_horizon(9)?;
    assert_eq!(s.tombstone_horizon(), 6);
    assert_eq!(
        s.contents().tombstone_horizon(),
        Some(9),
        "the store keeps the horizon asked for"
    );
    write(&s, Entry::deletion(b"k".to_vec(), 8))?;
    s.sweep_deletions()?;
    assert_eq!(
        held(&s, b"k"),
        vec![(8, true)],
        "8 is above the horizon in effect"
    );
    s.set_tombstone_horizon(2)?;
    assert_eq!(s.tombstone_horizon(), 2, "the horizon may move back");
    Ok(())
}

#[test]
fn test_a_corrupted_deletion_is_not_swept() -> TestResult {
    let s = store();
    write(&s, Entry::deletion(b"k".to_vec(), 4))?;
    s.corrupt(b"k", 4, Corruption::Truncate);
    s.set_discard_ts(9)?;
    s.sweep_deletions()?;
    let versions = s.contents().versions(b"k").to_vec();
    assert_eq!(
        (
            versions.len(),
            versions[0].version(),
            versions[0].is_decodable()
        ),
        (1, 4, false)
    );
    Ok(())
}

#[test]
fn test_a_deletion_above_the_tombstone_horizon_is_kept() -> TestResult {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    write(&s, Entry::deletion(b"k".to_vec(), 5))?;
    s.set_discard_ts(6)?;
    s.set_tombstone_horizon(4)?;
    s.compact()?;
    s.sweep_deletions()?;
    assert_eq!(held(&s, b"k"), vec![(5, true)]);
    Ok(())
}

#[test]
fn test_the_sweep_keeps_a_deletion_with_a_version_below_it() -> TestResult {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    write(&s, Entry::deletion(b"k".to_vec(), 4))?;
    s.set_discard_ts(6)?;
    s.sweep_deletions()?;
    assert_eq!(held(&s, b"k"), vec![(4, true), (2, false)]);
    Ok(())
}

#[test]
fn test_an_undecodable_item_survives_compaction() -> TestResult {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    write(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
    )?;
    s.corrupt(b"k", 2, Corruption::Truncate);
    s.set_discard_ts(6)?;
    s.compact()?;
    assert!(!s.needs_recovery(), "the store is not poisoned");
    let versions = s.contents().versions(b"k").to_vec();
    assert_eq!(versions.len(), 2);
    assert_eq!(
        (versions[0].version(), versions[0].is_decodable()),
        (5, true)
    );
    assert_eq!(
        (
            versions[1].version(),
            versions[1].is_decodable(),
            versions[1].value()
        ),
        (2, false, &[0x04][..])
    );
    Ok(())
}

#[test]
fn test_an_open_reader_holds_the_discard_timestamp_back() -> TestResult {
    let s = store();
    write(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    write(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
    )?;
    let reader = s.snapshot(3)?;
    s.set_discard_ts(6)?;
    assert_eq!(s.discard_ts(), 3);
    assert_eq!(s.tombstone_horizon(), 3);
    s.compact()?;
    assert_eq!(
        s.contents().versions(b"k").len(),
        2,
        "the version the reader sees survives"
    );
    assert_eq!(
        s.snapshot(3)?.read_ts(),
        3,
        "another reader at the held timestamp opens"
    );
    assert_eq!(s.snapshot(2).err(), Some(StoreError::TsTooOld()));
    drop(reader);
    assert_eq!(s.discard_ts(), 6);
    assert_eq!(s.snapshot(3).err(), Some(StoreError::TsTooOld()));
    s.compact()?;
    assert_eq!(
        held(&s, b"k"),
        vec![(5, false)],
        "released, the old version goes"
    );
    Ok(())
}

#[test]
fn test_the_discard_timestamp_never_decreases() -> TestResult {
    let s = store();
    s.set_discard_ts(6)?;
    s.set_discard_ts(4)?;
    assert_eq!(s.discard_ts(), 6);
    Ok(())
}

#[test]
fn test_the_initial_schema_a_drop_writes_survives_garbage_collection() -> TestResult {
    // A prefix drop fences the entries it writes: compaction never collects them, as on fjall, where a version the
    // drop removed can reach a compaction beneath a removal its filter is not shown. A drop of everything clears and
    // fences nothing.
    let s = store();
    let schema = b"\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x04name".to_vec();
    write(
        &s,
        Entry::put(schema.clone(), 5, 0x01, b"old".to_vec()).with_discard(),
    )?;
    s.drop_prefixes(
        7,
        &[schema[..9].to_vec()],
        vec![Entry::put(schema.clone(), 1, 0x01, b"initial".to_vec())],
    )?;
    write(
        &s,
        Entry::put(schema.clone(), 8, 0x01, b"new".to_vec()).with_discard(),
    )?;
    s.set_discard_ts(9)?;
    s.compact()?;
    assert_eq!(held(&s, &schema), vec![(8, false), (1, false)]);
    s.drop_all(vec![Entry::put(schema.clone(), 1, 0x01, b"root".to_vec())])?;
    write(
        &s,
        Entry::put(schema.clone(), 3, 0x01, b"new".to_vec()).with_discard(),
    )?;
    s.set_discard_ts(4)?;
    s.compact()?;
    assert_eq!(held(&s, &schema), vec![(3, false)]);
    Ok(())
}
