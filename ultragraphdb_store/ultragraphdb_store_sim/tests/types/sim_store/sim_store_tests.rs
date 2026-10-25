/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_sim::{Corruption, Fault, FaultConfig, OpRecord, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{
    Entry, PersistMode, RebuildRecord, Store, StoreSnapshot, WriteBatch,
};

fn store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

fn batch(key: &[u8], version: u64) -> WriteBatch {
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(key.to_vec(), version, 0x04, b"v".to_vec()));
    batch.set_applied_ts(version);
    batch
}

#[test]
fn test_clones_share_one_store_and_the_clock_given() {
    let clock = SimClock::new();
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), clock.clone());
    let clone = s.clone();
    clone.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    let item = s
        .get_at(b"k", 1)
        .unwrap()
        .expect("written through the clone");
    assert_eq!((item.version(), item.value()), (1, &b"v"[..]));
    clock.advance(5);
    assert_eq!((s.clock().now(), clone.clock().now()), (5, 5));
    s.crash();
    assert!(clone.needs_recovery(), "a crash reaches every clone");
}

#[test]
fn test_a_new_store_serves_at_once() {
    let s = store();
    assert!(!s.needs_recovery());
    assert!(s.take_log().is_empty());
    assert_eq!(
        (s.applied_ts(), s.discard_ts(), s.tombstone_horizon()),
        (0, 0, 0)
    );
    assert_eq!(s.rebuild_records().unwrap(), []);
}

#[test]
fn test_the_log_records_operations_in_order() {
    let s = store();
    s.sync().unwrap();
    let snapshot = s.snapshot(0).unwrap();
    let _ = snapshot.get(b"k");
    drop(snapshot);
    s.crash();
    s.recover();
    assert_eq!(
        s.take_log(),
        [
            OpRecord::Synced,
            OpRecord::SnapshotOpened { id: 0, read_ts: 0 },
            OpRecord::Read {
                id: 0,
                key: b"k".to_vec(),
                result: Ok(None)
            },
            OpRecord::SnapshotClosed { id: 0 },
            OpRecord::Crashed,
            OpRecord::Recovered,
        ]
    );
}

#[test]
fn test_the_log_records_each_operation_with_its_answer() {
    let s = store();
    s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    let mut big = WriteBatch::new();
    big.push(Entry::put(vec![1; 65_001], 1, 0, vec![]));
    let too_large = s.apply(big.clone(), PersistMode::Sync).unwrap_err();
    assert!(matches!(
        too_large.0,
        StoreErrorEnum::KeyTooLarge {
            size: 65_001,
            limit: 65_000,
            ..
        }
    ));
    s.ban_namespace(7).unwrap();
    s.set_discard_ts(2).unwrap();
    s.set_tombstone_horizon(1).unwrap();
    assert_eq!(s.snapshot(1).err(), Some(StoreError::TsTooOld()));
    s.compact().unwrap();
    s.sweep_deletions().unwrap();
    s.corrupt(b"k", 1, Corruption::Truncate);
    let record = RebuildRecord::new(4, vec![b"\x02p".to_vec()]);
    s.write_rebuild_record(&record).unwrap();
    s.drop_prefixes(5, &[b"\x02".to_vec()], Vec::new()).unwrap();
    s.inject(Fault::FailNextBatch);
    s.apply(batch(b"m", 3), PersistMode::Buffer).unwrap_err();
    assert_eq!(s.sync(), Err(StoreError::Poisoned()));
    s.power_loss();
    assert_eq!(s.snapshot(9).err(), Some(StoreError::Poisoned()));
    s.recover();
    assert_eq!(
        s.take_log(),
        [
            OpRecord::Applied {
                batch: batch(b"k", 1),
                mode: PersistMode::Buffer
            },
            OpRecord::Refused {
                batch: big,
                error: too_large
            },
            OpRecord::Banned { namespace: 7 },
            OpRecord::DiscardSet { ts: 2 },
            OpRecord::HorizonSet { ts: 1 },
            OpRecord::SnapshotRefused {
                read_ts: 1,
                error: StoreError::TsTooOld()
            },
            OpRecord::Compacted,
            OpRecord::Swept,
            OpRecord::Corrupted {
                key: b"k".to_vec(),
                version: 1,
                corruption: Corruption::Truncate
            },
            OpRecord::RebuildRecorded { record },
            OpRecord::Dropped {
                ts: 5,
                prefixes: Some(vec![b"\x02".to_vec()]),
                entries: Vec::new()
            },
            OpRecord::BatchFailed {
                batch: batch(b"m", 3)
            },
            OpRecord::WriteWhileFailed { operation: "sync" },
            OpRecord::PowerLost,
            OpRecord::SnapshotRefused {
                read_ts: 9,
                error: StoreError::Poisoned()
            },
            OpRecord::Recovered,
        ]
    );
}

#[test]
fn test_a_snapshot_of_an_earlier_process_releases_nothing() {
    let s = store();
    let old = s.snapshot(2).unwrap();
    s.crash();
    s.recover();
    let held = s.snapshot(4).unwrap();
    s.set_discard_ts(9).unwrap();
    assert_eq!(s.discard_ts(), 4);
    drop(old);
    assert_eq!(
        s.discard_ts(),
        4,
        "the old snapshot does not release the new one's hold"
    );
    assert_eq!(held.read_ts(), 4);
    assert!(
        !s.take_log()
            .iter()
            .any(|r| matches!(r, OpRecord::SnapshotClosed { id: 0 }))
    );
    drop(held);
    assert_eq!(s.discard_ts(), 9);
}

#[test]
fn test_two_snapshots_at_one_timestamp_hold_it_until_both_close() {
    let s = store();
    let (a, b) = (s.snapshot(3).unwrap(), s.snapshot(3).unwrap());
    s.set_discard_ts(8).unwrap();
    drop(a);
    assert_eq!(s.discard_ts(), 3);
    drop(b);
    assert_eq!(s.discard_ts(), 8);
}

#[test]
fn test_recovering_an_open_store_keeps_what_it_holds() {
    let s = store();
    s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    let before = s.contents();
    s.recover();
    assert_eq!(s.contents(), before);
    assert_eq!(s.take_log().last(), Some(&OpRecord::Recovered));
}

#[test]
fn test_a_crash_of_a_poisoned_store_decides_the_failed_batch_at_recovery() {
    // A crash or a power loss over a poisoned store keeps the failed batch in flight: recovery keeps it whole or
    // drops it whole, as the fault stream decides, so both happen over a few seeds.
    for power_loss in [false, true] {
        let outcomes: std::collections::BTreeSet<bool> = (0..16)
            .map(|seed| {
                let s = SimStore::new(Seed::new(seed), FaultConfig::none(), SimClock::new());
                s.inject(Fault::FailNextBatch);
                s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap_err();
                if power_loss {
                    s.power_loss();
                } else {
                    s.crash();
                }
                assert!(s.needs_recovery());
                s.recover();
                assert!(!s.needs_recovery());
                let kept = s.applied_ts() == 1;
                assert_eq!(s.contents().versions(b"k").len(), usize::from(kept));
                kept
            })
            .collect();
        assert_eq!(outcomes.len(), 2, "power loss {power_loss}");
    }
}

#[test]
fn test_contents_lists_deletions_and_the_metadata() {
    let s = store();
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"k".to_vec(), 2, 0x08, b"v".to_vec()).with_discard());
    batch.push(Entry::deletion(b"k".to_vec(), 3));
    batch.set_applied_ts(3);
    s.apply(batch, PersistMode::Buffer).unwrap();
    s.ban_namespace(7).unwrap();
    s.set_discard_ts(1).unwrap();
    s.set_tombstone_horizon(1).unwrap();
    let contents = s.contents();
    let versions = contents.versions(b"k");
    assert_eq!(versions.len(), 2);
    assert_eq!(
        (
            versions[0].version(),
            versions[0].is_deletion(),
            versions[0].discard(),
            versions[0].value()
        ),
        (3, true, false, &b""[..])
    );
    assert_eq!(
        (
            versions[1].version(),
            versions[1].meta(),
            versions[1].discard(),
            versions[1].is_deletion(),
            versions[1].value()
        ),
        (2, 0x08, true, false, &b"v"[..])
    );
    assert!(versions[0].is_decodable() && versions[1].is_decodable());
    assert_eq!(contents.applied_ts(), 3);
    assert_eq!(contents.banned().iter().collect::<Vec<_>>(), [&7]);
    assert_eq!(
        (contents.discard_ts(), contents.tombstone_horizon()),
        (1, Some(1))
    );
    assert!(contents.versions(b"none").is_empty());
}

#[test]
fn test_contents_show_an_undecodable_version_as_its_raw_bytes() {
    let s = store();
    let mut b = WriteBatch::new();
    b.push(Entry::put(b"k".to_vec(), 2, 0x08, b"value".to_vec()).with_discard());
    s.apply(b, PersistMode::Buffer).unwrap();
    s.corrupt(b"k", 2, Corruption::Truncate);
    let contents = s.contents();
    let version = &contents.versions(b"k")[0];
    assert_eq!(
        (
            version.version(),
            version.is_decodable(),
            version.meta(),
            version.discard(),
            version.is_deletion(),
            version.value()
        ),
        (2, false, 0, false, false, &[0x08][..]),
        "truncated to its meta byte"
    );
}
