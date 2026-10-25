/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The simulated store's rebuild records, and the crashes and power losses at each stage of a rebuild.

use std::collections::BTreeSet;

use ultragraphdb_store_sim::{
    Fault, FaultConfig, OpRecord, RebuildStage, Seed, SimClock, SimStore,
};
use ultragraphdb_traits_store::{Entry, PersistMode, RebuildRecord, Store, WriteBatch};

fn store(seed: u64) -> SimStore {
    SimStore::new(Seed::new(seed), FaultConfig::none(), SimClock::new())
}

fn record(id: u64) -> RebuildRecord {
    RebuildRecord::new(id, vec![b"\x00idx".to_vec(), b"\x04idx".to_vec()])
}

/// A batch writing `\x00k` at `version` and removing record `id`.
fn swap(id: u64, version: u64) -> WriteBatch {
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"\x00k".to_vec(), version, 0x08, vec![1]));
    batch.set_applied_ts(version);
    batch.remove_rebuild_record(id);
    batch
}

#[test]
fn test_a_record_is_held_until_a_batch_removes_it() {
    let s = store(1);
    s.write_rebuild_record(&record(4)).unwrap();
    s.write_rebuild_record(&record(9)).unwrap();
    assert_eq!(s.rebuild_records().unwrap(), [record(4), record(9)]);
    assert_eq!(s.contents().rebuild_records().len(), 2);
    s.apply(swap(4, 5), PersistMode::Buffer).unwrap();
    assert_eq!(s.rebuild_records().unwrap(), [record(9)]);
    let replaced = RebuildRecord::new(9, vec![b"\x00other".to_vec()]);
    s.write_rebuild_record(&replaced).unwrap();
    assert_eq!(s.rebuild_records().unwrap(), [replaced]);
}

#[test]
fn test_a_written_record_survives_a_crash_and_a_power_loss() {
    for power_loss in [false, true] {
        let s = store(2);
        s.write_rebuild_record(&record(4)).unwrap();
        if power_loss {
            s.power_loss();
        } else {
            s.crash();
        }
        s.recover();
        assert_eq!(s.rebuild_records().unwrap(), [record(4)]);
    }
}

#[test]
fn test_a_records_removal_survives_a_power_loss_with_its_batch_or_not_at_all() {
    let outcomes: BTreeSet<bool> = (0..16)
        .map(|seed| {
            let s = store(seed);
            s.write_rebuild_record(&record(4)).unwrap();
            s.apply(swap(4, 5), PersistMode::Buffer).unwrap();
            s.power_loss();
            s.recover();
            let removed = s.rebuild_records().unwrap().is_empty();
            assert_eq!(s.contents().versions(b"\x00k").len(), usize::from(removed));
            removed
        })
        .collect();
    assert_eq!(outcomes.len(), 2);
}

#[test]
fn test_dropping_everything_removes_every_record() {
    let s = store(1);
    s.write_rebuild_record(&record(4)).unwrap();
    s.drop_all(Vec::new()).unwrap();
    assert!(s.rebuild_records().unwrap().is_empty());
}

/// What a rebuild leaves after a crash, or a power loss when `power_loss`, at `stage`: whether its record survived,
/// over 16 seeds.
fn after_stop(stage: RebuildStage, power_loss: bool) -> BTreeSet<bool> {
    (0..16)
        .map(|seed| {
            let s = store(seed);
            s.inject(if power_loss {
                Fault::PowerLossDuringNextRebuild(stage)
            } else {
                Fault::CrashDuringNextRebuild(stage)
            });
            let written = s.write_rebuild_record(&record(4));
            assert_eq!(written.is_err(), stage <= RebuildStage::RecordSynced);
            if written.is_ok() {
                let t = s.create_temporary().unwrap();
                let mut temp = WriteBatch::new();
                temp.push(Entry::put(b"\x00idx1".to_vec(), 1, 0x04, vec![]));
                let built = s.apply_temporary(&t, temp);
                assert_eq!(built.is_err(), stage == RebuildStage::TemporaryWritten);
                if built.is_ok() {
                    assert!(s.apply(swap(4, 5), PersistMode::Buffer).is_err());
                }
            }
            assert!(s.needs_recovery(), "{stage:?} stops the rebuild");
            assert!(s.take_log().iter().any(|r| matches!(
                r,
                OpRecord::RebuildInterrupted { stage: at, power_loss: p, .. } if *at == stage && *p == power_loss
            )));
            s.recover();
            !s.rebuild_records().unwrap().is_empty()
        })
        .collect()
}

#[test]
fn test_a_rebuild_stopped_before_its_record_leaves_none() {
    assert_eq!(
        after_stop(RebuildStage::Start, false),
        BTreeSet::from([false])
    );
    assert_eq!(
        after_stop(RebuildStage::Start, true),
        BTreeSet::from([false])
    );
}

#[test]
fn test_a_record_written_and_not_synced_survives_a_crash_and_perhaps_a_power_loss() {
    assert_eq!(
        after_stop(RebuildStage::RecordWritten, false),
        BTreeSet::from([true])
    );
    assert_eq!(
        after_stop(RebuildStage::RecordWritten, true),
        BTreeSet::from([false, true])
    );
}

#[test]
fn test_a_synced_record_survives_every_later_stop() {
    for stage in [RebuildStage::RecordSynced, RebuildStage::TemporaryWritten] {
        for power_loss in [false, true] {
            assert_eq!(
                after_stop(stage, power_loss),
                BTreeSet::from([true]),
                "{stage:?}"
            );
        }
    }
}

#[test]
fn test_a_removal_handed_to_the_system_survives_a_crash_and_perhaps_a_power_loss() {
    assert_eq!(
        after_stop(RebuildStage::RecordRemoved, false),
        BTreeSet::from([false])
    );
    assert_eq!(
        after_stop(RebuildStage::RecordRemoved, true),
        BTreeSet::from([false, true])
    );
}

#[test]
fn test_drawn_rebuild_faults_stop_rebuilds() {
    for (faults, power_loss) in [
        (FaultConfig::none().with_crash_during_rebuild(1000), false),
        (
            FaultConfig::none().with_power_loss_during_rebuild(1000),
            true,
        ),
    ] {
        let stopped = (0..32).any(|seed| {
            let s = SimStore::new(Seed::new(seed), faults, SimClock::new());
            let _ = s.write_rebuild_record(&record(4));
            if !s.needs_recovery() {
                let _ = s
                    .create_temporary()
                    .and_then(|t| s.apply_temporary(&t, WriteBatch::new()));
                let _ = s.apply(swap(4, 5), PersistMode::Buffer);
            }
            s.take_log().iter().any(|r| {
                matches!(r, OpRecord::RebuildInterrupted { power_loss: p, .. } if *p == power_loss)
            })
        });
        assert!(stopped, "power loss {power_loss}");
    }
}
