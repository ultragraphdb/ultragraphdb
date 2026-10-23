/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{FaultConfig, OpRecord, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, StoreSnapshot, WriteBatch};

#[test]
fn test_clones_share_one_store_and_the_clock_given() {
    let clock = SimClock::new();
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), clock.clone());
    let clone = s.clone();
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"k".to_vec(), 1, 0, vec![]));
    clone.apply(batch, PersistMode::Buffer).unwrap();
    assert!(s.get_at(b"k", 1).unwrap().is_some());
    clock.advance(5);
    assert_eq!(s.clock().now(), 5);
}

#[test]
fn test_the_log_records_operations_in_order() {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    s.sync().unwrap();
    let snapshot = s.snapshot(0).unwrap();
    let _ = snapshot.get(b"k");
    drop(snapshot);
    s.crash();
    s.recover();
    let log = s.take_log();
    assert!(matches!(log[0], OpRecord::Synced));
    assert!(matches!(
        log[1],
        OpRecord::SnapshotOpened { id: 0, read_ts: 0 }
    ));
    assert!(matches!(log[2], OpRecord::Read { id: 0, .. }));
    assert!(matches!(log[3], OpRecord::SnapshotClosed { id: 0 }));
    assert_eq!(&log[4..], &[OpRecord::Crashed, OpRecord::Recovered]);
}

#[test]
fn test_a_snapshot_of_an_earlier_process_releases_nothing() {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
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
}

#[test]
fn test_contents_lists_deletions_and_the_metadata() {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
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
    assert_eq!(
        (versions[0].version(), versions[0].is_deletion()),
        (3, true)
    );
    assert_eq!(
        (
            versions[1].meta(),
            versions[1].discard(),
            versions[1].value()
        ),
        (0x08, true, &b"v"[..])
    );
    assert!(versions[1].is_decodable());
    assert_eq!(contents.applied_ts(), 3);
    assert!(contents.banned().contains(&7));
    assert_eq!(
        (contents.discard_ts(), contents.tombstone_horizon()),
        (1, Some(1))
    );
    assert!(contents.versions(b"none").is_empty());
}
