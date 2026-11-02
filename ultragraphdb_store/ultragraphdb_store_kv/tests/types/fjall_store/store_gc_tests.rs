/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Garbage collection below the discard timestamp on fjall: the simulated store's garbage-collection tests, and the
//! cases only fjall has, where a compaction's input holds what a removal or an open snapshot leaves behind.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::FjallStore;
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_test::{TestDir, held, write, write_raw};

fn one(store: &FjallStore, entry: Entry) -> TestResult {
    write(store, vec![entry])?;
    Ok(())
}

#[test]
fn test_versions_below_a_discard_point_are_collected() -> TestResult {
    let dir = TestDir::new("gc");
    let s = dir.open()?;
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"d2".to_vec()))?;
    one(&s, Entry::put(b"k".to_vec(), 4, 0x04, b"d4".to_vec()))?;
    one(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c5".to_vec()).with_discard(),
    )?;
    one(&s, Entry::put(b"k".to_vec(), 8, 0x04, b"d8".to_vec()))?;
    s.set_discard_ts(6)?;
    assert_eq!(
        held(&s, b"k")?,
        vec![(8, false), (5, false), (4, false), (2, false)],
        "nothing is collected before a compaction"
    );
    s.compact()?;
    assert_eq!(held(&s, b"k")?, vec![(8, false), (5, false)]);
    drop(s);
    assert_eq!(held(&dir.open()?, b"k")?, vec![(8, false), (5, false)]);
    Ok(())
}

#[test]
fn test_a_compaction_collects_across_tables_and_levels() -> TestResult {
    let dir = TestDir::new("gc-tables");
    let s = dir.open()?;
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"d2".to_vec()))?;
    s.compact()?;
    one(&s, Entry::put(b"k".to_vec(), 4, 0x04, b"d4".to_vec()))?;
    s.compact()?;
    // A rollup below the newest version, in the memtable while the older versions sit in the last level.
    one(&s, Entry::put(b"k".to_vec(), 9, 0x04, b"d9".to_vec()))?;
    one(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c5".to_vec()).with_discard(),
    )?;
    s.set_discard_ts(9)?;
    s.compact()?;
    assert_eq!(held(&s, b"k")?, vec![(9, false), (5, false)]);
    Ok(())
}

#[test]
fn test_a_deletion_is_never_collected_and_hides_what_is_below_it() -> TestResult {
    let dir = TestDir::new("gc-deletion");
    let s = dir.open()?;
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    one(&s, Entry::deletion(b"k".to_vec(), 4))?;
    s.set_discard_ts(6)?;
    s.compact()?;
    assert_eq!(held(&s, b"k")?, vec![(4, true)]);
    assert_eq!(s.get_at(b"k", 7)?, None);
    Ok(())
}

#[test]
fn test_the_sweep_removes_a_deletion_with_nothing_below_it() -> TestResult {
    let dir = TestDir::new("sweep");
    let s = dir.open()?;
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    one(&s, Entry::deletion(b"k".to_vec(), 4))?;
    one(&s, Entry::deletion(b"j".to_vec(), 3))?;
    s.set_discard_ts(6)?;
    assert_eq!(
        s.tombstone_horizon(),
        6,
        "the horizon follows the discard timestamp until it is set"
    );
    s.compact()?;
    s.sweep_deletions()?;
    assert!(held(&s, b"k")?.is_empty());
    assert!(held(&s, b"j")?.is_empty());
    drop(s);
    assert!(dir.open()?.dump()?.is_empty());
    Ok(())
}

#[test]
fn test_a_deletion_above_the_tombstone_horizon_is_kept() -> TestResult {
    let dir = TestDir::new("sweep-horizon");
    let s = dir.open()?;
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    one(&s, Entry::deletion(b"k".to_vec(), 5))?;
    s.set_discard_ts(6)?;
    s.set_tombstone_horizon(4)?;
    s.compact()?;
    s.sweep_deletions()?;
    assert_eq!(held(&s, b"k")?, vec![(5, true)]);
    Ok(())
}

#[test]
fn test_the_sweep_removes_a_deletion_at_the_horizon_and_keeps_one_above_it() -> TestResult {
    let dir = TestDir::new("sweep-at-horizon");
    let s = dir.open()?;
    one(&s, Entry::deletion(b"at".to_vec(), 4))?;
    one(&s, Entry::deletion(b"above".to_vec(), 5))?;
    s.set_discard_ts(6)?;
    s.set_tombstone_horizon(4)?;
    s.sweep_deletions()?;
    assert!(held(&s, b"at")?.is_empty());
    assert_eq!(held(&s, b"above")?, vec![(5, true)]);
    Ok(())
}

#[test]
fn test_the_sweep_judges_each_deletion_at_one_snapshot() -> TestResult {
    // At the sweep's snapshot the deletion at 4 has the deletion at 2 below it; only the oldest goes, and the next
    // sweep finds the deletion at 4 alone.
    let dir = TestDir::new("sweep-two");
    let s = dir.open()?;
    one(&s, Entry::deletion(b"k".to_vec(), 2))?;
    one(&s, Entry::deletion(b"k".to_vec(), 4))?;
    s.set_discard_ts(6)?;
    s.sweep_deletions()?;
    assert_eq!(held(&s, b"k")?, vec![(4, true)]);
    s.sweep_deletions()?;
    assert!(held(&s, b"k")?.is_empty());
    Ok(())
}

#[test]
fn test_the_sweep_keeps_a_deletion_with_a_version_below_it() -> TestResult {
    let dir = TestDir::new("sweep-below");
    let s = dir.open()?;
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    one(&s, Entry::deletion(b"k".to_vec(), 4))?;
    s.set_discard_ts(6)?;
    s.sweep_deletions()?;
    assert_eq!(held(&s, b"k")?, vec![(4, true), (2, false)]);
    Ok(())
}

#[test]
fn test_an_undecodable_item_survives_compaction() -> TestResult {
    let dir = TestDir::new("undecodable");
    let s = dir.open()?;
    one(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
    )?;
    drop(s);
    write_raw(&dir, b"k", 2, &[0x04])?;
    let reopened = dir.open()?;
    assert_eq!(
        reopened.undecodable_kept(),
        0,
        "no compaction has read it yet"
    );
    reopened.set_discard_ts(6)?;
    reopened.compact()?;
    assert!(!reopened.is_poisoned(), "the store is not poisoned");
    assert_eq!(
        reopened.undecodable_kept(),
        1,
        "the filter counted what it kept"
    );
    one(
        &reopened,
        Entry::put(b"later".to_vec(), 7, 0x04, Vec::new()),
    )?;
    let snapshot = reopened.snapshot(6)?;
    let versions: Vec<_> = snapshot.versions(b"k").collect();
    assert_eq!(
        versions,
        vec![
            Ok(Item::new(b"k".to_vec(), 5, 0x08, true, b"c".to_vec())),
            Err(StoreError::corrupted(
                b"k".to_vec(),
                "value too short to hold the version prefix".to_string()
            )),
        ],
        "the version and then the undecodable item's error"
    );
    Ok(())
}

#[test]
fn test_an_open_reader_holds_the_discard_timestamp_back() -> TestResult {
    let dir = TestDir::new("reader");
    let s = dir.open()?;
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()))?;
    one(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
    )?;
    let reader = s.snapshot(3)?;
    s.set_discard_ts(6)?;
    assert_eq!(s.discard_ts(), 3);
    assert_eq!(s.tombstone_horizon(), 3);
    s.compact()?;
    assert_eq!(
        held(&s, b"k")?,
        vec![(5, false), (2, false)],
        "the version the reader sees survives"
    );
    assert_eq!(reader.get(b"k")?.context("the key\'s item")?.version(), 2);
    assert_eq!(
        s.snapshot(3).map(|twin| twin.read_ts()),
        Ok(3),
        "another reader at the held timestamp opens"
    );
    drop(reader);
    assert_eq!(s.discard_ts(), 6);
    assert_eq!(s.snapshot(3).err(), Some(StoreError::ts_too_old()));
    s.compact()?;
    assert_eq!(held(&s, b"k")?, vec![(5, false)]);
    Ok(())
}

#[test]
fn test_the_discard_timestamp_never_decreases() -> TestResult {
    let dir = TestDir::new("discard-max");
    let s = dir.open()?;
    s.set_discard_ts(6)?;
    s.set_discard_ts(4)?;
    assert_eq!(s.discard_ts(), 6);
    drop(s);
    assert_eq!(dir.open()?.discard_ts(), 6);
    Ok(())
}

#[test]
fn test_a_drops_entries_survive_a_compaction_that_reads_the_versions_it_removed() -> TestResult {
    // A snapshot open at the flush keeps fjall from raising its garbage-collection watermark, so the versions the drop
    // removes reach the next compaction beneath lsm-tree removals the filter is not shown. Without the drop's fence,
    // the removed discard-flagged version at 5 would make the filter collect the initial schema written at 1.
    let dir = TestDir::new("fence");
    let s = dir.open()?;
    let schema = b"\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x04name".to_vec();
    one(
        &s,
        Entry::put(schema.clone(), 5, 0x01, b"old".to_vec()).with_discard(),
    )?;
    let held_open = s.snapshot(6)?;
    s.compact()?;
    s.drop_prefixes(
        7,
        &[schema[..9].to_vec()],
        vec![Entry::put(schema.clone(), 1, 0x01, b"initial".to_vec()).with_discard()],
    )?;
    s.set_discard_ts(6)?;
    s.compact()?;
    drop(held_open);
    let read = s.get_at(&schema, 8)?;
    assert_eq!(read.map(|i| i.into_value()), Some(b"initial".to_vec()));
    assert_eq!(held(&s, &schema)?, vec![(1, false)]);
    Ok(())
}

#[test]
fn test_an_open_snapshot_keeps_what_a_later_compaction_collects() -> TestResult {
    // lsm-tree keeps the table version an open snapshot reads, so an all-versions scan through it still reports what
    // a compaction collected after it opened, as a simulated snapshot does; a snapshot opened after does not.
    let dir = TestDir::new("all-versions");
    let s = dir.open()?;
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"d2".to_vec()))?;
    one(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c5".to_vec()).with_discard(),
    )?;
    let reader = s.snapshot(6)?;
    s.set_discard_ts(6)?;
    s.compact()?;
    let all = |snapshot: &ultragraphdb_store_kv::FjallSnapshot| -> TestResult<Vec<u64>> {
        Ok(snapshot
            .scan(&Scan::key(b"k".to_vec()).all_versions())
            .map(|i| i.map(|i| i.version()))
            .collect::<Result<_, _>>()?)
    };
    assert_eq!(all(&reader)?, vec![5, 2]);
    assert_eq!(all(&s.snapshot(6)?)?, vec![5]);
    Ok(())
}

#[test]
fn test_an_open_snapshot_reads_the_undecodable_version_a_later_drop_removes() -> TestResult {
    // The compaction keeps the undecodable version, and the snapshot open across the drop still reads its bytes, as
    // a simulated snapshot does; a snapshot opened after the drop reads nothing of the key.
    let dir = TestDir::new("drop-undecodable");
    let s = dir.open()?;
    write(
        &s,
        vec![
            Entry::put(b"pk".to_vec(), 1, 0x04, b"old".to_vec()),
            Entry::put(b"pk".to_vec(), 2, 0x08, b"rolled".to_vec()).with_discard(),
        ],
    )?;
    drop(s);
    write_raw(&dir, b"pk", 1, &[0x04])?;
    let reopened = dir.open()?;
    reopened.set_discard_ts(2)?;
    reopened.compact()?;
    let reader = reopened.snapshot(2)?;
    reopened.drop_prefixes(3, &[b"p".to_vec()], Vec::new())?;
    assert_eq!(
        reader.versions(b"pk").collect::<Vec<_>>(),
        vec![
            Ok(Item::new(b"pk".to_vec(), 2, 0x08, true, b"rolled".to_vec())),
            Err(StoreError::corrupted(
                b"pk".to_vec(),
                "value too short to hold the version prefix".to_string()
            )),
        ]
    );
    assert_eq!(reopened.snapshot(2)?.versions(b"pk").count(), 0);
    Ok(())
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the readers run on threads beside the writer, as the production store's readers do"
)]
fn test_a_batch_of_many_keys_is_seen_whole_by_concurrent_readers() -> TestResult {
    let dir = TestDir::new("whole");
    let s = std::sync::Arc::new(dir.open()?);
    let keys: Vec<Vec<u8>> = (0..10_000u32).map(|i| i.to_be_bytes().to_vec()).collect();
    std::thread::scope(|scope| -> TestResult {
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let (s, keys) = (s.clone(), keys.clone());
                scope.spawn(move || -> Result<Vec<(usize, usize)>, StoreError> {
                    let mut seen = Vec::new();
                    for _ in 0..50 {
                        let snapshot = s.snapshot(10)?;
                        let found = snapshot
                            .get_many(&keys)?
                            .iter()
                            .filter(|i| i.is_some())
                            .count();
                        let scanned = snapshot.scan(&Scan::prefix(Vec::new())).count();
                        seen.push((found, scanned));
                    }
                    Ok(seen)
                })
            })
            .collect();
        let mut batch = WriteBatch::new();
        keys.iter()
            .for_each(|k| batch.push(Entry::put(k.clone(), 3, 0x04, vec![1])));
        s.apply(batch, PersistMode::Buffer)?;
        for reader in readers {
            let seen = reader
                .join()
                .map_err(|_| TestError::new("a reader thread panicked"))??;
            for (found, scanned) in seen {
                assert!(
                    found == 0 || found == 10_000,
                    "a reader saw {found} of the batch's keys"
                );
                assert_eq!(found, scanned);
            }
        }
        Ok(())
    })?;
    Ok(())
}

#[test]
fn test_a_discard_point_at_the_discard_timestamp_cuts_and_one_above_it_does_not() -> TestResult {
    // Compaction collects what lies below the newest discard-flagged version or deletion at or below the discard
    // timestamp (the kv-store specification): 5 is at the discard timestamp 5, and 6 above it.
    let dir = TestDir::new("gc-at-discard");
    let s = dir.open()?;
    for (key, top) in [
        (
            b"at".as_slice(),
            Entry::put(b"at".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
        ),
        (
            b"above",
            Entry::put(b"above".to_vec(), 6, 0x08, b"c".to_vec()).with_discard(),
        ),
        (b"deleted", Entry::deletion(b"deleted".to_vec(), 5)),
    ] {
        one(&s, Entry::put(key.to_vec(), 1, 0x04, b"v1".to_vec()))?;
        one(&s, Entry::put(key.to_vec(), 2, 0x04, b"v2".to_vec()))?;
        one(&s, top)?;
    }
    s.set_discard_ts(5)?;
    s.compact()?;
    assert_eq!(held(&s, b"at")?, vec![(5, false)]);
    assert_eq!(
        held(&s, b"above")?,
        vec![(6, false), (2, false), (1, false)]
    );
    assert_eq!(held(&s, b"deleted")?, vec![(5, true)]);
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        held(&reopened, b"at")?,
        vec![(5, false)],
        "collected for good"
    );
    assert_eq!(
        reopened.get_at(b"above", 5)?.map(Item::into_value),
        Some(b"v2".to_vec())
    );
    Ok(())
}
