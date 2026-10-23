/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The shadow model, fed logs by hand: what it accepts and what it reports broken.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_sim::{
    Contents, FaultConfig, Invariant, OpRecord, Seed, SimClock, SimStore, StoreModel,
};
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

fn batch(entries: Vec<Entry>, applied: Option<u64>) -> WriteBatch {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    if let Some(ts) = applied {
        batch.set_applied_ts(ts);
    }
    batch
}

fn put(key: &[u8], version: u64) -> Entry {
    Entry::put(key.to_vec(), version, 0x04, vec![version as u8])
}

/// The model after feeding it the log of what `run` does to a fresh store, and the store.
fn observed(run: impl Fn(&SimStore)) -> (StoreModel, SimStore) {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    run(&store);
    let mut model = StoreModel::new();
    model
        .observe(&store.take_log())
        .expect("the log is consistent");
    model.check(&store.contents()).expect("the contents match");
    (model, store)
}

#[test]
fn test_a_store_that_kept_its_contract_passes() {
    observed(|s| {
        s.apply(
            batch(vec![put(b"a", 1), put(b"b", 2)], Some(2)),
            PersistMode::Sync,
        )
        .unwrap();
        let snapshot = s.snapshot(2).unwrap();
        let _ = snapshot.get(b"a");
        let _ = snapshot.get_many(&[b"a", b"b"]);
        let _ = snapshot
            .scan(&Scan::prefix(Vec::new()).all_versions())
            .count();
        drop(snapshot);
        s.drop_prefixes(3, &[b"a".to_vec()], Vec::new()).unwrap();
        s.set_discard_ts(2).unwrap();
        s.compact().unwrap();
        s.sweep_deletions().unwrap();
        s.power_loss();
    });
}

#[test]
fn test_contents_other_than_the_models_break_an_invariant() {
    let (mut model, _) = observed(|s| {
        s.apply(batch(vec![put(b"a", 1)], Some(1)), PersistMode::Buffer)
            .unwrap()
    });
    let breach = model.check(&Contents::default()).unwrap_err();
    assert_eq!(breach.invariant(), Invariant::AppliedTsIsHighest);

    let (mut model, _) = observed(|s| {
        s.apply(batch(vec![put(b"a", 1)], None), PersistMode::Buffer)
            .unwrap()
    });
    let breach = model.check(&Contents::default()).unwrap_err();
    assert_eq!(
        breach.invariant(),
        Invariant::ContentsMatchModel,
        "{breach}"
    );

    // The model saw a batch of two keys; the store holds one of them.
    let half = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    half.apply(batch(vec![put(b"a", 1)], None), PersistMode::Buffer)
        .unwrap();
    let mut model = StoreModel::new();
    let both = batch(vec![put(b"a", 1), put(b"b", 1)], None);
    model
        .observe(&[OpRecord::Applied {
            batch: both,
            mode: PersistMode::Buffer,
        }])
        .unwrap();
    let breach = model.check(&half.contents()).unwrap_err();
    assert_eq!(breach.invariant(), Invariant::BatchWhole, "{breach}");
}

#[test]
fn test_a_key_left_by_a_drop_breaks_the_drop_invariant() {
    let other = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    other
        .apply(batch(vec![put(b"pa", 1)], None), PersistMode::Buffer)
        .unwrap();
    let mut model = StoreModel::new();
    model
        .observe(&[OpRecord::Dropped {
            ts: 2,
            prefixes: Some(vec![b"p".to_vec()]),
            entries: Vec::new(),
        }])
        .unwrap();
    let breach = model.check(&other.contents()).unwrap_err();
    assert_eq!(breach.invariant(), Invariant::DroppedPrefixAbsent);
}

#[test]
fn test_a_write_below_a_deletion_breaks_its_invariant() {
    let mut model = StoreModel::new();
    let deletion = batch(vec![Entry::deletion(b"k".to_vec(), 9)], None);
    model
        .observe(&[OpRecord::Applied {
            batch: deletion,
            mode: PersistMode::Buffer,
        }])
        .unwrap();
    let below = batch(vec![put(b"k", 4)], None);
    let breach = model
        .observe(&[OpRecord::Applied {
            batch: below,
            mode: PersistMode::Buffer,
        }])
        .unwrap_err();
    assert_eq!(breach.invariant(), Invariant::NoWriteBelowDeletion);
}

#[test]
fn test_a_write_after_a_failure_breaks_the_recovery_invariant() {
    let mut model = StoreModel::new();
    let breach = model
        .observe(&[OpRecord::WriteWhileFailed { operation: "apply" }])
        .unwrap_err();
    assert_eq!(breach.invariant(), Invariant::RecoveryAfterFailure);
    assert!(breach.detail().contains("apply"));
}

#[test]
fn test_a_wrong_read_breaks_the_read_invariant() {
    let mut model = StoreModel::new();
    let written = batch(vec![put(b"k", 3)], None);
    let wrong = Item::new(b"k".to_vec(), 4, 0x04, false, vec![4]);
    let log = [
        OpRecord::Applied {
            batch: written,
            mode: PersistMode::Buffer,
        },
        OpRecord::SnapshotOpened { id: 0, read_ts: 5 },
        OpRecord::Read {
            id: 0,
            key: b"k".to_vec(),
            result: Ok(Some(wrong)),
        },
    ];
    assert_eq!(
        model.observe(&log).unwrap_err().invariant(),
        Invariant::ReadsAtOrBelowTimestamp
    );
}

#[test]
fn test_a_wrong_scan_or_failure_breaks_the_read_invariant() {
    let mut model = StoreModel::new();
    let log = [
        OpRecord::Applied {
            batch: batch(vec![put(b"a", 1), put(b"b", 1)], None),
            mode: PersistMode::Buffer,
        },
        OpRecord::SnapshotOpened { id: 0, read_ts: 1 },
    ];
    model.observe(&log).unwrap();
    let a = Item::new(b"a".to_vec(), 1, 0x04, false, vec![1]);
    let partial = OpRecord::Scanned {
        id: 0,
        scan: Scan::prefix(Vec::new()),
        items: vec![Ok(a.clone())],
    };
    assert_eq!(
        model.observe(&[partial]).unwrap_err().invariant(),
        Invariant::ReadsAtOrBelowTimestamp
    );
    let failed = StoreError::ReadFailed(b"b".to_vec(), "x".into());
    let cut = OpRecord::Scanned {
        id: 0,
        scan: Scan::prefix(Vec::new()),
        items: vec![Ok(a), Err(failed)],
    };
    model.observe(&[cut]).expect("a scan may fail partway");
    let corrupted = OpRecord::Read {
        id: 0,
        key: b"a".to_vec(),
        result: Err(StoreError::Corrupted(b"a".to_vec(), "x".into())),
    };
    assert_eq!(
        model.observe(&[corrupted]).unwrap_err().invariant(),
        Invariant::ReadsAtOrBelowTimestamp
    );
}

#[test]
fn test_a_snapshot_below_the_discard_timestamp_breaks_its_invariant() {
    let mut model = StoreModel::new();
    model.observe(&[OpRecord::DiscardSet { ts: 5 }]).unwrap();
    model
        .observe(&[OpRecord::SnapshotRefused {
            read_ts: 4,
            error: StoreError::TsTooOld(),
        }])
        .unwrap();
    let wrongly = OpRecord::SnapshotRefused {
        read_ts: 6,
        error: StoreError::TsTooOld(),
    };
    assert_eq!(
        model.observe(&[wrongly]).unwrap_err().invariant(),
        Invariant::DiscardBelowOpenReads
    );
    let opened = OpRecord::SnapshotOpened { id: 0, read_ts: 4 };
    assert_eq!(
        model.observe(&[opened]).unwrap_err().invariant(),
        Invariant::DiscardBelowOpenReads
    );
}

#[test]
fn test_a_lost_synced_batch_breaks_its_invariant() {
    let mut model = StoreModel::new();
    let synced = batch(vec![put(b"a", 1)], None);
    let log = [
        OpRecord::Applied {
            batch: synced,
            mode: PersistMode::Sync,
        },
        OpRecord::PowerLost,
        OpRecord::Recovered,
    ];
    model.observe(&log).unwrap();
    let breach = model.check(&Contents::default()).unwrap_err();
    assert_eq!(
        breach.invariant(),
        Invariant::SyncedBatchSurvives,
        "{breach}"
    );
}

#[test]
fn test_survivors_out_of_order_break_the_prefix_invariant() {
    // The second unsynced batch survives without the first.
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    store
        .apply(batch(vec![put(b"b", 2)], None), PersistMode::Buffer)
        .unwrap();
    let mut model = StoreModel::new();
    let log = [
        OpRecord::Applied {
            batch: batch(vec![put(b"a", 1)], None),
            mode: PersistMode::Buffer,
        },
        OpRecord::Applied {
            batch: batch(vec![put(b"b", 2)], None),
            mode: PersistMode::Buffer,
        },
        OpRecord::PowerLost,
        OpRecord::Recovered,
    ];
    model.observe(&log).unwrap();
    let breach = model.check(&store.contents()).unwrap_err();
    assert_eq!(
        breach.invariant(),
        Invariant::SurvivorsArePrefix,
        "{breach}"
    );
}

#[test]
fn test_a_record_after_an_unchecked_recovery_is_refused() {
    let mut model = StoreModel::new();
    let breach = model
        .observe(&[OpRecord::Crashed, OpRecord::Recovered, OpRecord::Synced])
        .unwrap_err();
    assert!(breach.detail().contains("follows a recovery"));
}
