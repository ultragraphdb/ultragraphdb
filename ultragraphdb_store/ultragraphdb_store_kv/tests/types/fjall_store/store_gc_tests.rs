/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Garbage collection below the discard timestamp on fjall: the simulated store's garbage-collection tests, and the
//! cases only fjall has, where a compaction's input holds what a removal or an open snapshot leaves behind.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::FjallStore;
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

use crate::test_store::{TestDir, held, write, write_raw};

fn one(store: &FjallStore, entry: Entry) {
    write(store, vec![entry]);
}

#[test]
fn test_versions_below_a_discard_point_are_collected() {
    let dir = TestDir::new("gc");
    let s = dir.open();
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"d2".to_vec()));
    one(&s, Entry::put(b"k".to_vec(), 4, 0x04, b"d4".to_vec()));
    one(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c5".to_vec()).with_discard(),
    );
    one(&s, Entry::put(b"k".to_vec(), 8, 0x04, b"d8".to_vec()));
    s.set_discard_ts(6).unwrap();
    assert_eq!(
        held(&s, b"k"),
        vec![(8, false), (5, false), (4, false), (2, false)],
        "nothing is collected before a compaction"
    );
    s.compact().unwrap();
    assert_eq!(held(&s, b"k"), vec![(8, false), (5, false)]);
    drop(s);
    assert_eq!(held(&dir.open(), b"k"), vec![(8, false), (5, false)]);
}

#[test]
fn test_a_compaction_collects_across_tables_and_levels() {
    let dir = TestDir::new("gc-tables");
    let s = dir.open();
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"d2".to_vec()));
    s.compact().unwrap();
    one(&s, Entry::put(b"k".to_vec(), 4, 0x04, b"d4".to_vec()));
    s.compact().unwrap();
    // A rollup below the newest version, in the memtable while the older versions sit in the last level.
    one(&s, Entry::put(b"k".to_vec(), 9, 0x04, b"d9".to_vec()));
    one(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c5".to_vec()).with_discard(),
    );
    s.set_discard_ts(9).unwrap();
    s.compact().unwrap();
    assert_eq!(held(&s, b"k"), vec![(9, false), (5, false)]);
}

#[test]
fn test_a_deletion_is_never_collected_and_hides_what_is_below_it() {
    let dir = TestDir::new("gc-deletion");
    let s = dir.open();
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    one(&s, Entry::deletion(b"k".to_vec(), 4));
    s.set_discard_ts(6).unwrap();
    s.compact().unwrap();
    assert_eq!(held(&s, b"k"), vec![(4, true)]);
    assert_eq!(s.get_at(b"k", 7).unwrap(), None);
}

#[test]
fn test_the_sweep_removes_a_deletion_with_nothing_below_it() {
    let dir = TestDir::new("sweep");
    let s = dir.open();
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    one(&s, Entry::deletion(b"k".to_vec(), 4));
    one(&s, Entry::deletion(b"j".to_vec(), 3));
    s.set_discard_ts(6).unwrap();
    assert_eq!(
        s.tombstone_horizon(),
        6,
        "the horizon follows the discard timestamp until it is set"
    );
    s.compact().unwrap();
    s.sweep_deletions().unwrap();
    assert!(held(&s, b"k").is_empty());
    assert!(held(&s, b"j").is_empty());
    drop(s);
    assert!(dir.open().dump().unwrap().is_empty());
}

#[test]
fn test_a_deletion_above_the_tombstone_horizon_is_kept() {
    let dir = TestDir::new("sweep-horizon");
    let s = dir.open();
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    one(&s, Entry::deletion(b"k".to_vec(), 5));
    s.set_discard_ts(6).unwrap();
    s.set_tombstone_horizon(4).unwrap();
    s.compact().unwrap();
    s.sweep_deletions().unwrap();
    assert_eq!(held(&s, b"k"), vec![(5, true)]);
}

#[test]
fn test_the_sweep_removes_a_deletion_at_the_horizon_and_keeps_one_above_it() {
    let dir = TestDir::new("sweep-at-horizon");
    let s = dir.open();
    one(&s, Entry::deletion(b"at".to_vec(), 4));
    one(&s, Entry::deletion(b"above".to_vec(), 5));
    s.set_discard_ts(6).unwrap();
    s.set_tombstone_horizon(4).unwrap();
    s.sweep_deletions().unwrap();
    assert!(held(&s, b"at").is_empty());
    assert_eq!(held(&s, b"above"), vec![(5, true)]);
}

#[test]
fn test_the_sweep_judges_each_deletion_at_one_snapshot() {
    // At the sweep's snapshot the deletion at 4 has the deletion at 2 below it; only the oldest goes, and the next
    // sweep finds the deletion at 4 alone.
    let dir = TestDir::new("sweep-two");
    let s = dir.open();
    one(&s, Entry::deletion(b"k".to_vec(), 2));
    one(&s, Entry::deletion(b"k".to_vec(), 4));
    s.set_discard_ts(6).unwrap();
    s.sweep_deletions().unwrap();
    assert_eq!(held(&s, b"k"), vec![(4, true)]);
    s.sweep_deletions().unwrap();
    assert!(held(&s, b"k").is_empty());
}

#[test]
fn test_the_sweep_keeps_a_deletion_with_a_version_below_it() {
    let dir = TestDir::new("sweep-below");
    let s = dir.open();
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    one(&s, Entry::deletion(b"k".to_vec(), 4));
    s.set_discard_ts(6).unwrap();
    s.sweep_deletions().unwrap();
    assert_eq!(held(&s, b"k"), vec![(4, true), (2, false)]);
}

#[test]
fn test_an_undecodable_item_survives_compaction() {
    let dir = TestDir::new("undecodable");
    let s = dir.open();
    one(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
    );
    drop(s);
    write_raw(&dir, b"k", 2, &[0x04]);
    let s = dir.open();
    s.set_discard_ts(6).unwrap();
    s.compact().unwrap();
    assert!(!s.is_poisoned(), "the store is not poisoned");
    assert_eq!(s.undecodable_kept(), 1, "the filter counted what it kept");
    one(&s, Entry::put(b"later".to_vec(), 7, 0x04, Vec::new()));
    let snapshot = s.snapshot(6).unwrap();
    let versions: Vec<_> = snapshot.versions(b"k").collect();
    assert_eq!(
        versions,
        vec![
            Ok(Item::new(b"k".to_vec(), 5, 0x08, true, b"c".to_vec())),
            Err(StoreError::Corrupted(
                b"k".to_vec(),
                "value too short to hold the version prefix".to_string()
            )),
        ],
        "the version and then the undecodable item's error"
    );
}

#[test]
fn test_an_open_reader_holds_the_discard_timestamp_back() {
    let dir = TestDir::new("reader");
    let s = dir.open();
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"v".to_vec()));
    one(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
    );
    let reader = s.snapshot(3).unwrap();
    s.set_discard_ts(6).unwrap();
    assert_eq!(s.discard_ts(), 3);
    assert_eq!(s.tombstone_horizon(), 3);
    s.compact().unwrap();
    assert_eq!(
        held(&s, b"k"),
        vec![(5, false), (2, false)],
        "the version the reader sees survives"
    );
    assert_eq!(reader.get(b"k").unwrap().unwrap().version(), 2);
    assert_eq!(
        s.snapshot(3).map(|twin| twin.read_ts()),
        Ok(3),
        "another reader at the held timestamp opens"
    );
    drop(reader);
    assert_eq!(s.discard_ts(), 6);
    assert_eq!(s.snapshot(3).err(), Some(StoreError::TsTooOld()));
    s.compact().unwrap();
    assert_eq!(held(&s, b"k"), vec![(5, false)]);
}

#[test]
fn test_the_discard_timestamp_never_decreases() {
    let dir = TestDir::new("discard-max");
    let s = dir.open();
    s.set_discard_ts(6).unwrap();
    s.set_discard_ts(4).unwrap();
    assert_eq!(s.discard_ts(), 6);
    drop(s);
    assert_eq!(dir.open().discard_ts(), 6);
}

#[test]
fn test_a_drops_entries_survive_a_compaction_that_reads_the_versions_it_removed() {
    // A snapshot open at the flush keeps fjall from raising its garbage-collection watermark, so the versions the drop
    // removes reach the next compaction beneath lsm-tree removals the filter is not shown. Without the drop's fence,
    // the removed discard-flagged version at 5 would make the filter collect the initial schema written at 1.
    let dir = TestDir::new("fence");
    let s = dir.open();
    let schema = b"\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x04name".to_vec();
    one(
        &s,
        Entry::put(schema.clone(), 5, 0x01, b"old".to_vec()).with_discard(),
    );
    let held_open = s.snapshot(6).unwrap();
    s.compact().unwrap();
    s.drop_prefixes(
        7,
        &[schema[..9].to_vec()],
        vec![Entry::put(schema.clone(), 1, 0x01, b"initial".to_vec()).with_discard()],
    )
    .unwrap();
    s.set_discard_ts(6).unwrap();
    s.compact().unwrap();
    drop(held_open);
    let read = s.get_at(&schema, 8).unwrap();
    assert_eq!(read.map(|i| i.into_value()), Some(b"initial".to_vec()));
    assert_eq!(held(&s, &schema), vec![(1, false)]);
}

#[test]
fn test_an_open_snapshot_keeps_what_a_later_compaction_collects() {
    // lsm-tree keeps the table version an open snapshot reads, so an all-versions scan through it still reports what
    // a compaction collected after it opened, as a simulated snapshot does; a snapshot opened after does not.
    let dir = TestDir::new("all-versions");
    let s = dir.open();
    one(&s, Entry::put(b"k".to_vec(), 2, 0x04, b"d2".to_vec()));
    one(
        &s,
        Entry::put(b"k".to_vec(), 5, 0x08, b"c5".to_vec()).with_discard(),
    );
    let reader = s.snapshot(6).unwrap();
    s.set_discard_ts(6).unwrap();
    s.compact().unwrap();
    let all = |snapshot: &ultragraphdb_store_kv::FjallSnapshot| -> Vec<u64> {
        snapshot
            .scan(&Scan::key(b"k".to_vec()).all_versions())
            .map(|i| i.unwrap().version())
            .collect()
    };
    assert_eq!(all(&reader), vec![5, 2]);
    assert_eq!(all(&s.snapshot(6).unwrap()), vec![5]);
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the readers run on threads beside the writer, as the production store's readers do"
)]
fn test_a_batch_of_many_keys_is_seen_whole_by_concurrent_readers() {
    let dir = TestDir::new("whole");
    let s = std::sync::Arc::new(dir.open());
    let keys: Vec<Vec<u8>> = (0..10_000u32).map(|i| i.to_be_bytes().to_vec()).collect();
    std::thread::scope(|scope| {
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let (s, keys) = (s.clone(), keys.clone());
                scope.spawn(move || {
                    let mut seen = Vec::new();
                    for _ in 0..50 {
                        let snapshot = s.snapshot(10).unwrap();
                        let found = snapshot
                            .get_many(&keys)
                            .unwrap()
                            .iter()
                            .filter(|i| i.is_some())
                            .count();
                        let scanned = snapshot.scan(&Scan::prefix(Vec::new())).count();
                        seen.push((found, scanned));
                    }
                    seen
                })
            })
            .collect();
        let mut batch = WriteBatch::new();
        keys.iter()
            .for_each(|k| batch.push(Entry::put(k.clone(), 3, 0x04, vec![1])));
        s.apply(batch, PersistMode::Buffer).unwrap();
        for reader in readers {
            for (found, scanned) in reader.join().unwrap() {
                assert!(
                    found == 0 || found == 10_000,
                    "a reader saw {found} of the batch's keys"
                );
                assert_eq!(found, scanned);
            }
        }
    });
}
