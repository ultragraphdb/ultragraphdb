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
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::AppliedTsIsHighest,
            "applied timestamp 0 where 1 was expected"
        )
    );

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
    assert!(
        breach
            .detail()
            .starts_with("key 61 holds [] where [StoredVersion { version: 1,"),
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
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::BatchWhole,
            "a batch survives with 1 of its 2 entries; key 62 at 1 is missing"
        )
    );
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
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::DroppedPrefixAbsent,
            "key 7061 survived the drop covering it"
        )
    );
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
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::NoWriteBelowDeletion,
            "key 6b written at 4 below its deletion at 9"
        )
    );
    // At the deletion's own version, and above it, a write is allowed.
    let at_and_above = batch(vec![put(b"k", 9), put(b"k", 10)], None);
    model
        .observe(&[OpRecord::Applied {
            batch: at_and_above,
            mode: PersistMode::Buffer,
        }])
        .expect("a write at or above the deletion");
}

#[test]
fn test_a_write_after_a_failure_breaks_the_recovery_invariant() {
    let mut model = StoreModel::new();
    let breach = model
        .observe(&[OpRecord::WriteWhileFailed { operation: "apply" }])
        .unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::RecoveryAfterFailure,
            "apply was called on a store that had to be recovered first"
        )
    );
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
    let breach = model.observe(&log).unwrap_err();
    assert_eq!(breach.invariant(), Invariant::ReadsAtOrBelowTimestamp);
    assert!(
        breach
            .detail()
            .starts_with("a read of key 6b at 5 returned Some("),
        "{breach}"
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
    let breach = model.observe(&[corrupted]).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::ReadsAtOrBelowTimestamp,
            "a read of 61 failed with Corrupted value of key 61: x"
        )
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
    let breach = model.observe(&[wrongly]).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::DiscardBelowOpenReads,
            "a snapshot at 6 was refused with Transaction is too old"
        )
    );
    let opened = OpRecord::SnapshotOpened { id: 0, read_ts: 4 };
    let breach = model.observe(&[opened]).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::DiscardBelowOpenReads,
            "a snapshot opened at 4, below the discard timestamp 5"
        )
    );
    model
        .observe(&[OpRecord::SnapshotOpened { id: 1, read_ts: 5 }])
        .expect("a snapshot at the discard timestamp opens");
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
    assert!(
        breach.detail().starts_with(
            "a synced version of key 61 is missing after recovery: key 61 holds [] where"
        ),
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
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::ContentsMatchModel,
            "record 2 of the log follows a recovery the model has not checked"
        )
    );
}

#[test]
fn test_the_model_keeps_what_a_prefix_drop_wrote_through_a_compaction() {
    let (_, store) = observed(|s| {
        s.drop_prefixes(
            3,
            &[b"\x01".to_vec()],
            vec![Entry::put(b"\x01s".to_vec(), 1, 0x01, b"initial".to_vec())],
        )
        .unwrap();
        s.apply(
            batch(
                vec![Entry::put(b"\x01s".to_vec(), 4, 0x01, b"new".to_vec()).with_discard()],
                Some(4),
            ),
            PersistMode::Buffer,
        )
        .unwrap();
        s.set_discard_ts(5).unwrap();
        s.compact().unwrap();
    });
    assert_eq!(
        store.contents().versions(b"\x01s").len(),
        2,
        "the fenced version stays"
    );
}

/// A deletion a power loss took with its batch never happened: a write below its version afterwards breaks no
/// invariant. The posting lists' counter restarts at the applied timestamp the store kept, so a lost batch's
/// versions are written again.
#[test]
fn test_a_deletion_a_power_loss_lost_holds_no_write_back() {
    let mut lost = 0;
    for seed in 1..=20 {
        let store = SimStore::new(Seed::new(seed), FaultConfig::none(), SimClock::new());
        let mut model = StoreModel::new();
        store
            .apply(batch(vec![put(b"k", 5)], Some(5)), PersistMode::Sync)
            .unwrap();
        store
            .apply(
                batch(vec![Entry::deletion(b"k".to_vec(), 9)], Some(9)),
                PersistMode::Buffer,
            )
            .unwrap();
        store.power_loss();
        model.observe(&store.take_log()).unwrap();
        model.check(&store.contents()).unwrap();
        store.recover();
        model.observe(&store.take_log()).unwrap();
        model.check(&store.contents()).unwrap();
        let kept = store
            .contents()
            .versions(b"k")
            .iter()
            .any(|v| v.version() == 9);
        store
            .apply(batch(vec![put(b"k", 7)], Some(7)), PersistMode::Buffer)
            .unwrap_or(());
        let outcome = model.observe(&store.take_log());
        if kept {
            assert_eq!(
                outcome.unwrap_err().invariant(),
                Invariant::NoWriteBelowDeletion
            );
        } else {
            outcome.unwrap();
            lost += 1;
        }
    }
    assert!(lost > 0, "a power loss took the deletion's batch");
}

/// The contents of a fresh store after `batches` are applied, each buffered.
fn holding(batches: Vec<WriteBatch>) -> Contents {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    batches
        .into_iter()
        .for_each(|b| store.apply(b, PersistMode::Buffer).unwrap());
    store.contents()
}

/// After a power loss, as after a crash with a batch in flight, a batch the store holds in part is reported as a
/// partial batch, not as a lost or out-of-order survivor.
#[test]
fn test_a_batch_held_in_part_after_a_recovery_breaks_the_whole_batch_invariant() {
    let both = || batch(vec![put(b"a", 1), put(b"b", 1)], None);
    let logs = [
        (
            "power loss",
            vec![
                OpRecord::Applied {
                    batch: both(),
                    mode: PersistMode::Buffer,
                },
                OpRecord::PowerLost,
                OpRecord::Recovered,
            ],
        ),
        (
            "crash in flight",
            vec![
                OpRecord::CrashedDuringBatch { batch: both() },
                OpRecord::Recovered,
            ],
        ),
        (
            "failed batch",
            vec![OpRecord::BatchFailed { batch: both() }, OpRecord::Recovered],
        ),
    ];
    for (name, log) in logs {
        let mut model = StoreModel::new();
        model.observe(&log).unwrap();
        let breach = model
            .check(&holding(vec![batch(vec![put(b"a", 1)], None)]))
            .unwrap_err();
        assert_eq!(
            (breach.invariant(), breach.detail()),
            (
                Invariant::BatchWhole,
                "a batch survives with 1 of its 2 entries; key 62 at 1 is missing"
            ),
            "{name}"
        );
    }
}

/// Feeds the model what the store logged since the last call and compares their contents.
fn checked(store: &SimStore, model: &mut StoreModel) {
    model
        .observe(&store.take_log())
        .expect("the log is consistent");
    model.check(&store.contents()).expect("the contents match");
}

/// The versions `contents` holds of `key`, newest first, each with whether it is a deletion.
fn versions(contents: &Contents, key: &[u8]) -> Vec<(u64, bool)> {
    contents
        .versions(key)
        .iter()
        .map(|v| (v.version(), v.is_deletion()))
        .collect()
}

/// A crash with a batch in flight, or a failed batch, may keep the batch whole or lose it whole; a power loss may
/// keep any prefix of the unsynced batches. The model accepts each, adopts it and checks the next step against it.
#[test]
fn test_a_recovery_accepts_every_outcome_its_failure_allows() {
    let both = || batch(vec![put(b"a", 1), put(b"b", 1)], None);
    let in_flight = [
        OpRecord::CrashedDuringBatch { batch: both() },
        OpRecord::BatchFailed { batch: both() },
    ];
    for failure in in_flight {
        for kept in [vec![], vec![both()]] {
            let mut model = StoreModel::new();
            model
                .observe(&[failure.clone(), OpRecord::Recovered])
                .unwrap();
            let contents = holding(kept.clone());
            model.check(&contents).expect("an allowed outcome");
            // The adopted outcome is what the next step is checked against.
            model.observe(&[]).unwrap();
            model.check(&contents).expect("the adopted outcome");
            let other = holding(if kept.is_empty() {
                vec![both()]
            } else {
                vec![]
            });
            assert_eq!(
                model.check(&other).unwrap_err().invariant(),
                Invariant::ContentsMatchModel
            );
        }
    }
    let three: Vec<WriteBatch> = (1..=3).map(|v| batch(vec![put(b"k", v)], None)).collect();
    for survivors in 0..=3 {
        let mut model = StoreModel::new();
        let mut log: Vec<OpRecord> = three
            .iter()
            .map(|b| OpRecord::Applied {
                batch: b.clone(),
                mode: PersistMode::Buffer,
            })
            .collect();
        log.extend([OpRecord::PowerLost, OpRecord::Recovered]);
        model.observe(&log).unwrap();
        model
            .check(&holding(three[..survivors].to_vec()))
            .unwrap_or_else(|b| panic!("{survivors} survivors: {b}"));
    }
}

/// A recovery that ends at an applied timestamp no outcome holds breaks the applied-timestamp invariant.
#[test]
fn test_a_recovery_at_a_wrong_applied_timestamp_breaks_its_invariant() {
    let mut model = StoreModel::new();
    model
        .observe(&[
            OpRecord::Applied {
                batch: batch(vec![put(b"a", 1)], Some(1)),
                mode: PersistMode::Sync,
            },
            OpRecord::Crashed,
            OpRecord::Recovered,
        ])
        .unwrap();
    let breach = model
        .check(&holding(vec![batch(vec![put(b"a", 1)], Some(2))]))
        .unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::AppliedTsIsHighest,
            "applied timestamp 2 where 1 was expected"
        )
    );
}

/// A snapshot is refused as poisoned only while the process is down, and with no other error.
#[test]
fn test_a_snapshot_refused_without_cause_breaks_its_invariant() {
    let poisoned = |read_ts| OpRecord::SnapshotRefused {
        read_ts,
        error: StoreError::Poisoned(),
    };
    let breach = StoreModel::new().observe(&[poisoned(3)]).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::DiscardBelowOpenReads,
            "a snapshot at 3 was refused with The store is poisoned by an earlier failed write and must be reopened"
        )
    );
    StoreModel::new()
        .observe(&[OpRecord::Crashed, poisoned(3)])
        .expect("a store whose process is down refuses snapshots");
    let other = OpRecord::SnapshotRefused {
        read_ts: 3,
        error: StoreError::Unsupported("snapshots"),
    };
    let breach = StoreModel::new().observe(&[other]).unwrap_err();
    assert_eq!(
        breach.detail(),
        "a snapshot at 3 was refused with The store does not support snapshots"
    );
}

/// A point read or a read of many keys may fail as the fault stream makes it, or as corrupted once the key's bytes
/// were; any other failure, or a wrong item, breaks the read invariant. A read through an unknown or closed snapshot
/// is not checked.
#[test]
fn test_reads_fail_only_as_the_faults_allow() {
    let mut model = StoreModel::new();
    model
        .observe(&[
            OpRecord::Applied {
                batch: batch(vec![put(b"a", 1), put(b"b", 2)], None),
                mode: PersistMode::Buffer,
            },
            OpRecord::SnapshotOpened { id: 0, read_ts: 1 },
        ])
        .unwrap();
    let a = Item::new(b"a".to_vec(), 1, 0x04, false, vec![1]);
    let read = |key: &[u8], result| OpRecord::Read {
        id: 0,
        key: key.to_vec(),
        result,
    };
    model
        .observe(&[
            read(b"a", Ok(Some(a.clone()))),
            read(b"b", Ok(None)),
            read(b"a", Err(StoreError::ReadFailed(b"a".to_vec(), "x".into()))),
            OpRecord::ReadMany {
                id: 0,
                keys: vec![b"a".to_vec(), b"b".to_vec()],
                result: Ok(vec![Some(a.clone()), None]),
            },
            OpRecord::ReadMany {
                id: 0,
                keys: vec![b"a".to_vec()],
                result: Err(StoreError::ReadFailed(b"a".to_vec(), "x".into())),
            },
        ])
        .expect("right items and drawn failures");
    let wrong = [
        (
            read(b"a", Err(StoreError::Unsupported("reads"))),
            "a read of 61 failed with The store does not support reads",
        ),
        (
            OpRecord::ReadMany {
                id: 0,
                keys: vec![b"a".to_vec(), b"b".to_vec()],
                result: Err(StoreError::Corrupted(b"b".to_vec(), "x".into())),
            },
            "a read of 61, 62 failed with Corrupted value of key 62: x",
        ),
    ];
    for (record, detail) in wrong {
        let breach = model.clone().observe(&[record]).unwrap_err();
        assert_eq!(
            (breach.invariant(), breach.detail()),
            (Invariant::ReadsAtOrBelowTimestamp, detail)
        );
    }
    // The version at 2 is above the snapshot's timestamp.
    let above = Item::new(b"b".to_vec(), 2, 0x04, false, vec![2]);
    let breach = model
        .clone()
        .observe(&[OpRecord::ReadMany {
            id: 0,
            keys: vec![b"a".to_vec(), b"b".to_vec()],
            result: Ok(vec![Some(a.clone()), Some(above.clone())]),
        }])
        .unwrap_err();
    assert!(
        breach
            .detail()
            .starts_with("a read of key 62 at 1 returned Some("),
        "{breach}"
    );
    // Once a key's bytes are corrupted, its reads are not compared and may fail as corrupted.
    model
        .observe(&[
            OpRecord::Corrupted {
                key: b"a".to_vec(),
                version: 1,
                corruption: ultragraphdb_store_sim::Corruption::Truncate,
            },
            read(b"a", Err(StoreError::Corrupted(b"a".to_vec(), "x".into()))),
            OpRecord::Corrupted {
                key: b"a".to_vec(),
                version: 1,
                corruption: ultragraphdb_store_sim::Corruption::FlipValueByte,
            },
            read(b"a", Ok(None)),
            OpRecord::Read {
                id: 9,
                key: b"b".to_vec(),
                result: Ok(Some(above.clone())),
            },
            OpRecord::SnapshotClosed { id: 0 },
            read(b"b", Ok(Some(above))),
        ])
        .expect("tainted keys and unknown snapshots are not compared");
}

/// A temporary keyspace's snapshot reads every version written to it, and a batch to a keyspace that does not exist
/// breaks the contents invariant.
#[test]
fn test_a_temporary_keyspace_is_read_at_every_version() {
    let breach = StoreModel::new()
        .observe(&[OpRecord::TemporaryApplied {
            temporary: 3,
            batch: batch(vec![put(b"k", 1)], None),
        }])
        .unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::ContentsMatchModel,
            "a batch was written to temporary keyspace 3, which does not exist"
        )
    );
    let mut model = StoreModel::new();
    model
        .observe(&[
            OpRecord::TemporaryCreated { temporary: 1 },
            OpRecord::TemporaryApplied {
                temporary: 1,
                batch: batch(vec![put(b"k", 3), put(b"k", 7)], None),
            },
            OpRecord::TemporarySnapshotOpened {
                id: 5,
                temporary: 1,
            },
        ])
        .unwrap();
    let at = |version: u64| OpRecord::Read {
        id: 5,
        key: b"k".to_vec(),
        result: Ok(Some(Item::new(
            b"k".to_vec(),
            version,
            0x04,
            false,
            vec![version as u8],
        ))),
    };
    model.observe(&[at(7)]).expect("the newest version");
    assert_eq!(
        model.clone().observe(&[at(3)]).unwrap_err().invariant(),
        Invariant::ReadsAtOrBelowTimestamp
    );
    model
        .observe(&[OpRecord::SnapshotClosed { id: 5 }, at(3)])
        .expect("a closed snapshot's reads are not compared");
    // The temporary keyspace is not part of the store's contents.
    model.check(&Contents::default()).unwrap();
}

/// Compaction collects below the discard timestamp in effect, which an open snapshot holds down; the sweep removes a
/// deletion at or below the tombstone horizon with nothing below it. The store and the model agree at every step, on
/// the versions computed here by hand.
#[test]
fn test_garbage_collection_keeps_what_open_reads_and_the_horizon_need() {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut model = StoreModel::new();
    let rolled = Entry::put(b"k".to_vec(), 2, 0x08, vec![2]).with_discard();
    store
        .apply(
            batch(
                vec![
                    put(b"k", 1),
                    rolled,
                    put(b"k", 3),
                    Entry::deletion(b"d".to_vec(), 1),
                    put(b"e", 1),
                    Entry::deletion(b"e".to_vec(), 2),
                ],
                Some(3),
            ),
            PersistMode::Buffer,
        )
        .unwrap();
    let snapshot = store.snapshot(1).unwrap();
    store.set_discard_ts(3).unwrap();
    store.compact().unwrap();
    checked(&store, &mut model);
    assert_eq!(
        versions(&store.contents(), b"k"),
        vec![(3, false), (2, false), (1, false)],
        "the snapshot at 1 holds the discard timestamp at 1"
    );
    drop(snapshot);
    store.compact().unwrap();
    checked(&store, &mut model);
    assert_eq!(
        versions(&store.contents(), b"k"),
        vec![(3, false), (2, false)]
    );
    assert_eq!(
        versions(&store.contents(), b"e"),
        vec![(2, true)],
        "a deletion collects the values below it"
    );
    store.set_tombstone_horizon(0).unwrap();
    store.sweep_deletions().unwrap();
    checked(&store, &mut model);
    assert_eq!(versions(&store.contents(), b"d"), vec![(1, true)]);
    store.set_tombstone_horizon(2).unwrap();
    store.sweep_deletions().unwrap();
    checked(&store, &mut model);
    assert!(store.contents().versions(b"d").is_empty());
    assert!(store.contents().versions(b"e").is_empty());
}

/// A ban drops the later entries addressed to the namespace and hides its keys from snapshots opened after it.
#[test]
fn test_a_banned_namespace_takes_no_writes_and_reads_empty() {
    let mut key = vec![0x00];
    key.extend_from_slice(&5u64.to_be_bytes());
    key.push(b'x');
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut model = StoreModel::new();
    store
        .apply(batch(vec![put(&key, 1)], None), PersistMode::Buffer)
        .unwrap();
    store.ban_namespace(5).unwrap();
    store
        .apply(
            batch(vec![put(&key, 2), put(b"k", 2)], None),
            PersistMode::Buffer,
        )
        .unwrap();
    let snapshot = store.snapshot(2).unwrap();
    assert_eq!(snapshot.get(&key), Ok(None));
    drop(snapshot);
    checked(&store, &mut model);
    let contents = store.contents();
    assert_eq!(versions(&contents, &key), vec![(1, false)]);
    assert_eq!(versions(&contents, b"k"), vec![(2, false)]);
    assert_eq!(
        contents.banned().iter().copied().collect::<Vec<_>>(),
        vec![5]
    );
}

/// A drop stopped by a crash at any stage, or by a power loss, is accepted by the model as the store recovers it: a
/// crash keeps the record once it was written, and recovery finishes the drop; a crash before it leaves the data.
#[test]
fn test_an_interrupted_drop_recovers_as_the_model_expects() {
    use ultragraphdb_store_sim::{DropStage, Fault};
    for stage in DropStage::ALL {
        for power_loss in [false, true] {
            let mut outcomes = std::collections::BTreeSet::new();
            for seed in 0..8 {
                let store = SimStore::new(Seed::new(seed), FaultConfig::none(), SimClock::new());
                let mut model = StoreModel::new();
                store
                    .apply(
                        batch(vec![put(b"pa", 1), put(b"q", 1)], Some(1)),
                        PersistMode::Sync,
                    )
                    .unwrap();
                store.inject(if power_loss {
                    Fault::PowerLossDuringNextDrop(stage)
                } else {
                    Fault::CrashDuringNextDrop(stage)
                });
                let stopped = store.drop_prefixes(2, &[b"p".to_vec()], Vec::new());
                assert_eq!(
                    stopped,
                    Err(StoreError::WriteFailed(format!(
                        "simulated {} during a drop",
                        if power_loss {
                            "power loss"
                        } else {
                            "process crash"
                        }
                    )))
                );
                checked(&store, &mut model);
                store.recover();
                checked(&store, &mut model);
                let contents = store.contents();
                assert_eq!(versions(&contents, b"q"), vec![(1, false)]);
                outcomes.insert(contents.versions(b"pa").is_empty());
            }
            let expected: Vec<bool> = match (stage, power_loss) {
                (DropStage::Start, _) => vec![false],
                (DropStage::RecordWritten, true) => vec![false, true],
                _ => vec![true],
            };
            assert_eq!(
                outcomes.into_iter().collect::<Vec<_>>(),
                expected,
                "{stage:?}, power loss {power_loss}"
            );
        }
    }
}

/// A power loss while a prefix drop's record is unsynced may keep the drop or lose it. When the drop rewrites the one
/// version under its prefix, both outcomes hold the same contents, and only the fence tells them apart: a later
/// compaction collects the version the drop never wrote and keeps the one it fenced. The model follows both outcomes
/// until the compaction shows which one the store took.
#[test]
fn test_a_recovery_the_contents_cannot_decide_is_decided_by_a_later_compaction() {
    use ultragraphdb_store_sim::{DropStage, Fault};
    let key = b"\x01s".to_vec();
    let schema = || Entry::put(key.clone(), 1, 0x01, b"name: string .".to_vec());
    let mut kept = std::collections::BTreeSet::new();
    for seed in 0..8 {
        let store = SimStore::new(Seed::new(seed), FaultConfig::none(), SimClock::new());
        let mut model = StoreModel::new();
        store
            .apply(batch(vec![schema()], Some(1)), PersistMode::Sync)
            .unwrap();
        store.inject(Fault::PowerLossDuringNextDrop(DropStage::RecordWritten));
        let _ = store.drop_prefixes(2, &[b"\x01".to_vec()], vec![schema()]);
        checked(&store, &mut model);
        store.recover();
        checked(&store, &mut model);
        assert_eq!(versions(&store.contents(), &key), vec![(1, false)]);
        // Contents neither outcome holds still break the invariant.
        let other = holding(vec![batch(
            vec![Entry::put(key.clone(), 1, 0x01, b"x".to_vec())],
            Some(1),
        )]);
        let breach = model.clone().check(&other).unwrap_err();
        assert_eq!(
            (breach.invariant(), breach.detail()),
            (
                Invariant::ContentsMatchModel,
                "key 0173 holds [StoredVersion { version: 1, meta: 1, discard: false, deletion: false, \
                 decodable: true, value: [120] }] where [StoredVersion { version: 1, meta: 1, discard: false, \
                 deletion: false, decodable: true, value: [110, 97, 109, 101, 58, 32, 115, 116, 114, 105, 110, 103, \
                 32, 46] }] was expected"
            )
        );
        store
            .apply(
                batch(
                    vec![Entry::put(key.clone(), 4, 0x08, b"rolled".to_vec()).with_discard()],
                    Some(4),
                ),
                PersistMode::Buffer,
            )
            .unwrap();
        store.set_discard_ts(5).unwrap();
        store.compact().unwrap();
        model
            .observe(&store.take_log())
            .unwrap_or_else(|b| panic!("seed {seed}: {b}"));
        model
            .check(&store.contents())
            .unwrap_or_else(|b| panic!("seed {seed}: {b}"));
        kept.insert(versions(&store.contents(), &key));
    }
    assert_eq!(
        kept.into_iter().collect::<Vec<_>>(),
        vec![vec![(4, false)], vec![(4, false), (1, false)]],
        "the drop is lost on some seeds and kept on others"
    );
}

/// A compaction keeps an undecodable version the model collects, and a snapshot opened then reads it after a drop
/// removed it: the model accepts the corrupted read, through the snapshot that opened while the key was tainted.
#[test]
fn test_a_snapshot_opened_before_a_drop_reads_the_corrupted_version_the_drop_removed() {
    let store = SimStore::new(Seed::new(0), FaultConfig::none(), SimClock::new());
    let mut model = StoreModel::new();
    let key = b"pk".to_vec();
    store
        .apply(
            batch(
                vec![
                    Entry::put(key.clone(), 1, 0x04, b"old".to_vec()),
                    Entry::put(key.clone(), 2, 0x08, b"rolled".to_vec()).with_discard(),
                ],
                Some(2),
            ),
            PersistMode::Buffer,
        )
        .unwrap();
    store.corrupt(&key, 1, ultragraphdb_store_sim::Corruption::Truncate);
    store.set_discard_ts(2).unwrap();
    store.compact().unwrap();
    let reader = store.snapshot(2).unwrap();
    store
        .drop_prefixes(3, &[b"p".to_vec()], Vec::new())
        .unwrap();
    checked(&store, &mut model);
    let read: Vec<_> = reader.versions(&key).collect();
    assert_eq!(
        read,
        vec![
            Ok(Item::new(key.clone(), 2, 0x08, true, b"rolled".to_vec())),
            Err(StoreError::Corrupted(
                key.clone(),
                "value too short to hold the version prefix".to_string()
            )),
        ]
    );
    assert_eq!(model.observe(&store.take_log()), Ok(()));
}

/// A snapshot keeps the taint of every key corrupted while it was open or before it opened, through a drop that
/// lifts the taint for later snapshots; a snapshot opened after the drop compares the key again.
#[test]
fn test_a_snapshot_keeps_its_taint_through_a_drop() {
    let corrupted = |id| OpRecord::Read {
        id,
        key: b"a".to_vec(),
        result: Err(StoreError::Corrupted(b"a".to_vec(), "x".into())),
    };
    let mut model = StoreModel::new();
    model
        .observe(&[
            OpRecord::Applied {
                batch: batch(vec![put(b"a", 1)], Some(1)),
                mode: PersistMode::Buffer,
            },
            OpRecord::SnapshotOpened { id: 0, read_ts: 1 },
            OpRecord::Corrupted {
                key: b"a".to_vec(),
                version: 1,
                corruption: ultragraphdb_store_sim::Corruption::Truncate,
            },
            OpRecord::SnapshotOpened { id: 1, read_ts: 1 },
            OpRecord::Dropped {
                ts: 2,
                prefixes: Some(vec![b"a".to_vec()]),
                entries: Vec::new(),
            },
            OpRecord::SnapshotOpened { id: 2, read_ts: 1 },
            corrupted(0),
            corrupted(1),
        ])
        .expect("the snapshots open before the drop read the key as tainted");
    let breach = model.observe(&[corrupted(2)]).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::ReadsAtOrBelowTimestamp,
            "a read of 61 failed with Corrupted value of key 61: x"
        )
    );
}

/// A Truncate corruption of `key` at `version`, as the store logs it.
fn truncated(key: &[u8], version: u64) -> OpRecord {
    OpRecord::Corrupted {
        key: key.to_vec(),
        version,
        corruption: ultragraphdb_store_sim::Corruption::Truncate,
    }
}

/// The error a read of a truncated version of `key` fails with.
fn undecodable(key: &[u8]) -> StoreError {
    StoreError::Corrupted(
        key.to_vec(),
        "value too short to hold the version prefix".to_string(),
    )
}

/// The log of a store whose snapshot heals: snapshot 0 reads the corrupted version of `a`, a drop then removes the
/// version from the store, and the same snapshot reads it again decoded cleanly, as a snapshot reading the store's
/// live corruptions would. The key is tainted, so its reads are not compared with the model's state; the second
/// read differs from the first, and that breaks the repeat invariant, for a point read and for a scan alike.
#[test]
fn test_a_snapshot_whose_answer_changes_breaks_the_repeat_invariant() {
    let a1 = Item::new(b"a".to_vec(), 1, 0x04, false, vec![1]);
    let versions = Scan::key(b"a".to_vec()).all_versions();
    let mut model = StoreModel::new();
    model
        .observe(&[
            OpRecord::Applied {
                batch: batch(vec![put(b"a", 1)], Some(1)),
                mode: PersistMode::Buffer,
            },
            OpRecord::SnapshotOpened { id: 0, read_ts: 1 },
            truncated(b"a", 1),
            OpRecord::Read {
                id: 0,
                key: b"a".to_vec(),
                result: Err(undecodable(b"a")),
            },
            OpRecord::Scanned {
                id: 0,
                scan: versions.clone(),
                items: vec![Err(undecodable(b"a"))],
            },
            OpRecord::Dropped {
                ts: 2,
                prefixes: Some(vec![b"a".to_vec()]),
                entries: Vec::new(),
            },
        ])
        .expect("the snapshot read the corrupted version once");
    let healed_read = OpRecord::Read {
        id: 0,
        key: b"a".to_vec(),
        result: Ok(Some(a1.clone())),
    };
    let breach = model.clone().observe(&[healed_read]).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::SnapshotReadsRepeat,
            "snapshot 0 read key 61 as Ok(Some(Item { key: [97], version: 1, meta: 4, discard: false, value: [1] })) \
             where it read Err(StoreError(Corrupted { key: [97], reason: \"value too short to hold the version \
             prefix\" })) the first time"
        )
    );
    let healed_scan = OpRecord::Scanned {
        id: 0,
        scan: versions,
        items: vec![Ok(a1)],
    };
    let breach = model.observe(&[healed_scan]).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::SnapshotReadsRepeat,
            "snapshot 0 scanned Scan { start: [97], end: Some([97, 0]), seek: None, direction: Forward, \
             all_versions: true } as [Ok(Item { key: [97], version: 1, meta: 4, discard: false, value: [1] })] \
             where it scanned [Err(StoreError(Corrupted { key: [97], reason: \"value too short to hold the version \
             prefix\" }))] the first time"
        )
    );
}

/// A repeated read may differ from the first only where the store may legitimately answer anew: a read the fault
/// stream failed, compared only up to its failure, and the reads covering a key corrupted since. The answers are a
/// snapshot's own, shared by its point reads and the keys of its many-key reads, and die with it.
#[test]
fn test_a_repeated_read_differs_only_by_a_failure_or_a_new_corruption() {
    let a1 = Item::new(b"a".to_vec(), 1, 0x04, false, vec![1]);
    let flipped = Item::new(b"b".to_vec(), 1, 0x04, false, vec![!1]);
    let failed = |key: &[u8]| StoreError::ReadFailed(key.to_vec(), "x".into());
    let read = |key: &[u8], result| OpRecord::Read {
        id: 0,
        key: key.to_vec(),
        result,
    };
    let scan = |items| OpRecord::Scanned {
        id: 0,
        scan: Scan::prefix(Vec::new()),
        items,
    };
    let mut model = StoreModel::new();
    model
        .observe(&[
            OpRecord::Applied {
                batch: batch(vec![put(b"a", 1), put(b"b", 1)], Some(1)),
                mode: PersistMode::Buffer,
            },
            OpRecord::SnapshotOpened { id: 0, read_ts: 1 },
            read(b"a", Ok(Some(a1.clone()))),
            read(b"a", Err(failed(b"a"))),
            truncated(b"b", 1),
            OpRecord::ReadMany {
                id: 0,
                keys: vec![b"a".to_vec(), b"b".to_vec()],
                result: Ok(vec![Some(a1.clone()), Some(flipped.clone())]),
            },
            OpRecord::ReadMany {
                id: 0,
                keys: vec![b"b".to_vec()],
                result: Err(failed(b"b")),
            },
            read(b"b", Ok(Some(flipped.clone()))),
            scan(vec![Ok(a1.clone()), Err(undecodable(b"b"))]),
            scan(vec![Ok(a1.clone()), Err(failed(b"b"))]),
            scan(vec![Err(failed(b""))]),
        ])
        .expect("the same answers, and failures the faults drew");
    let wrong = [
        (
            read(b"b", Err(undecodable(b"b"))),
            "snapshot 0 read key 62 as Err(StoreError(Corrupted { key: [98], reason: \"value too short to hold the \
             version prefix\" })) where it read Ok(Some(Item { key: [98], version: 1, meta: 4, discard: false, \
             value: [254] })) the first time",
        ),
        (
            OpRecord::ReadMany {
                id: 0,
                keys: vec![b"b".to_vec()],
                result: Ok(vec![None]),
            },
            "snapshot 0 read key 62 as Ok(None) where it read Ok(Some(Item { key: [98], version: 1, meta: 4, \
             discard: false, value: [254] })) the first time",
        ),
        (
            scan(vec![Ok(a1.clone()), Ok(flipped.clone()), Err(failed(b"b"))]),
            "snapshot 0 scanned Scan { start: [], end: None, seek: None, direction: Forward, all_versions: false } \
             as [Ok(Item { key: [97], version: 1, meta: 4, discard: false, value: [1] }), Ok(Item { key: [98], \
             version: 1, meta: 4, discard: false, value: [254] })] before a read failure, where it scanned \
             [Ok(Item { key: [97], version: 1, meta: 4, discard: false, value: [1] }), Err(StoreError(Corrupted { \
             key: [98], reason: \"value too short to hold the version prefix\" }))] the first time",
        ),
    ];
    for (record, detail) in wrong {
        let breach = model.clone().observe(&[record]).unwrap_err();
        assert_eq!(
            (breach.invariant(), breach.detail()),
            (Invariant::SnapshotReadsRepeat, detail)
        );
    }
    // A corruption of `b` lets every read covering it answer anew; a snapshot opened later has answers of its own,
    // and a closed snapshot's reads are compared with nothing.
    model
        .observe(&[
            truncated(b"b", 1),
            read(b"b", Err(undecodable(b"b"))),
            scan(vec![Ok(a1.clone()), Ok(flipped.clone())]),
            OpRecord::SnapshotOpened { id: 1, read_ts: 1 },
            OpRecord::Read {
                id: 1,
                key: b"b".to_vec(),
                result: Ok(Some(flipped.clone())),
            },
            OpRecord::SnapshotClosed { id: 0 },
            read(b"b", Ok(Some(flipped))),
        ])
        .expect("answers a corruption, another snapshot or a close set free");
}

/// A key a many-key read names twice is read twice in one call, and a corruption drawn at the second read is logged
/// before the call, so the first occurrence read the bytes before it. The call is compared with the snapshot's
/// earlier answers, not with itself, and the last occurrence of a key is the answer later reads repeat.
#[test]
fn test_a_many_key_read_holds_the_last_answer_to_a_key_it_names_twice() {
    let b1 = Item::new(b"b".to_vec(), 1, 0x04, false, vec![1]);
    let flipped = Item::new(b"b".to_vec(), 1, 0x04, false, vec![!1]);
    let flip = OpRecord::Corrupted {
        key: b"b".to_vec(),
        version: 1,
        corruption: ultragraphdb_store_sim::Corruption::FlipValueByte,
    };
    let read = |result| OpRecord::Read {
        id: 0,
        key: b"b".to_vec(),
        result,
    };
    let mut model = StoreModel::new();
    model
        .observe(&[
            OpRecord::Applied {
                batch: batch(vec![put(b"b", 1)], Some(1)),
                mode: PersistMode::Buffer,
            },
            OpRecord::SnapshotOpened { id: 0, read_ts: 1 },
            truncated(b"b", 1),
            read(Ok(Some(b1.clone()))),
            flip,
            OpRecord::ReadMany {
                id: 0,
                keys: vec![b"b".to_vec(), b"b".to_vec()],
                result: Ok(vec![Some(b1.clone()), Some(flipped.clone())]),
            },
            read(Ok(Some(flipped))),
        ])
        .expect("one call read the key before and after the corruption drawn during it");
    let breach = model.observe(&[read(Ok(Some(b1)))]).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::SnapshotReadsRepeat,
            "snapshot 0 read key 62 as Ok(Some(Item { key: [98], version: 1, meta: 4, discard: false, value: \
             [1] })) where it read Ok(Some(Item { key: [98], version: 1, meta: 4, discard: false, value: [254] })) \
             the first time"
        )
    );
}

/// A snapshot holds the answers of its first 64 reads: a read past them is compared with nothing.
#[test]
fn test_a_snapshot_holds_the_answers_of_64_reads() {
    let key = |n: u8| vec![b'k', n];
    let read = |key: Vec<u8>, result| OpRecord::Read { id: 0, key, result };
    let mut log = vec![
        OpRecord::SnapshotOpened { id: 0, read_ts: 1 },
        truncated(&key(0), 1),
        truncated(&key(64), 1),
        read(key(0), Err(undecodable(&key(0)))),
    ];
    log.extend((1..64).map(|n| read(key(n), Ok(None))));
    log.push(read(key(64), Err(undecodable(&key(64)))));
    log.push(read(key(64), Ok(None)));
    let mut model = StoreModel::new();
    model
        .observe(&log)
        .expect("the 65th read is not held, so its answer may change");
    let breach = model.observe(&[read(key(0), Ok(None))]).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::SnapshotReadsRepeat,
            "snapshot 0 read key 6b00 as Ok(None) where it read Err(StoreError(Corrupted { key: [107, 0], reason: \
             \"value too short to hold the version prefix\" })) the first time"
        )
    );
}

/// A corruption lets a snapshot answer anew only where its reads reach the corrupted version: a snapshot of the data
/// at or above the version. A snapshot below it, and a snapshot of a temporary keyspace, whose bytes are never
/// corrupted, must still repeat their answers to the key.
#[test]
fn test_a_corruption_frees_only_the_snapshots_that_read_its_version() {
    let item = |version: u64| Item::new(b"a".to_vec(), version, 0x04, false, vec![version as u8]);
    let read = |id, result| OpRecord::Read {
        id,
        key: b"a".to_vec(),
        result,
    };
    let mut model = StoreModel::new();
    model
        .observe(&[
            OpRecord::Applied {
                batch: batch(vec![put(b"a", 1), put(b"a", 2)], Some(2)),
                mode: PersistMode::Buffer,
            },
            OpRecord::TemporaryCreated { temporary: 1 },
            OpRecord::TemporaryApplied {
                temporary: 1,
                batch: batch(vec![put(b"a", 1)], None),
            },
            OpRecord::SnapshotOpened { id: 0, read_ts: 1 },
            OpRecord::SnapshotOpened { id: 1, read_ts: 2 },
            OpRecord::TemporarySnapshotOpened {
                id: 2,
                temporary: 1,
            },
            read(0, Ok(Some(item(1)))),
            read(1, Ok(Some(item(2)))),
            read(2, Ok(Some(item(1)))),
            truncated(b"a", 2),
            read(1, Err(undecodable(b"a"))),
        ])
        .expect("the snapshot at the corrupted version reads it anew");
    for (id, detail) in [
        (
            0,
            "snapshot 0 read key 61 as Ok(None) where it read Ok(Some(Item { key: [97], version: 1, meta: 4, \
             discard: false, value: [1] })) the first time",
        ),
        (
            2,
            "snapshot 2 read key 61 as Ok(None) where it read Ok(Some(Item { key: [97], version: 1, meta: 4, \
             discard: false, value: [1] })) the first time",
        ),
    ] {
        let breach = model.clone().observe(&[read(id, Ok(None))]).unwrap_err();
        assert_eq!(
            (breach.invariant(), breach.detail()),
            (Invariant::SnapshotReadsRepeat, detail)
        );
    }
}
