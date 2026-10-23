/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The fault classes of the simulated store.

use std::collections::BTreeSet;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_sim::{
    Corruption, DropStage, Fault, FaultConfig, OpRecord, Seed, SimClock, SimStore,
};
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

fn store(seed: u64, faults: FaultConfig) -> SimStore {
    SimStore::new(Seed::new(seed), faults, SimClock::new())
}

fn batch(key: &[u8], version: u64) -> WriteBatch {
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(key.to_vec(), version, 0x04, b"value".to_vec()));
    batch.set_applied_ts(version);
    batch
}

#[test]
fn test_a_crash_keeps_every_batch_handed_to_the_system() {
    let s = store(1, FaultConfig::none());
    (1..=5).for_each(|v| s.apply(batch(b"k", v), PersistMode::Buffer).unwrap());
    s.crash();
    assert!(s.needs_recovery());
    assert_eq!(
        s.apply(batch(b"k", 6), PersistMode::Buffer),
        Err(StoreError::Poisoned())
    );
    assert_eq!(
        s.snapshot(1).err(),
        Some(StoreError::Poisoned()),
        "a process that is down serves no read"
    );
    s.recover();
    assert_eq!(s.applied_ts(), 5);
    assert_eq!(s.contents().versions(b"k").len(), 5);
}

#[test]
fn test_a_crash_during_a_batch_keeps_it_whole_or_not_at_all() {
    let outcomes: BTreeSet<usize> = (0..16)
        .map(|seed| {
            let s = store(seed, FaultConfig::none());
            s.apply(batch(b"a", 1), PersistMode::Buffer).unwrap();
            s.inject(Fault::CrashDuringNextBatch);
            let mut two = batch(b"b", 2);
            two.push(Entry::put(b"c".to_vec(), 2, 0x04, vec![]));
            assert!(s.apply(two, PersistMode::Buffer).is_err());
            s.recover();
            let held = [&b"b"[..], b"c"]
                .iter()
                .filter(|k| !s.contents().versions(k).is_empty())
                .count();
            assert!(held == 0 || held == 2);
            assert_eq!(s.contents().versions(b"a").len(), 1);
            held
        })
        .collect();
    assert_eq!(outcomes.len(), 2);
}

#[test]
fn test_a_power_loss_keeps_the_synced_batches_and_a_prefix_of_the_rest() {
    let survivors: BTreeSet<u64> = (0..24)
        .map(|seed| {
            let s = store(seed, FaultConfig::none());
            (1..=3).for_each(|v| s.apply(batch(b"k", v), PersistMode::Buffer).unwrap());
            s.sync().unwrap();
            (4..=8).for_each(|v| s.apply(batch(b"k", v), PersistMode::Buffer).unwrap());
            s.power_loss();
            s.recover();
            let versions: Vec<u64> = s
                .contents()
                .versions(b"k")
                .iter()
                .map(|v| v.version())
                .collect();
            let kept = versions.len() as u64;
            assert_eq!(
                versions,
                (1..=kept).rev().collect::<Vec<_>>(),
                "a prefix of batch order"
            );
            assert!(kept >= 3);
            assert_eq!(s.applied_ts(), kept);
            kept
        })
        .collect();
    assert!(survivors.len() >= 4, "{survivors:?}");
}

#[test]
fn test_a_power_loss_after_recovery_keeps_what_survived_the_first() {
    let s = store(3, FaultConfig::none());
    (1..=4).for_each(|v| s.apply(batch(b"k", v), PersistMode::Buffer).unwrap());
    s.power_loss();
    s.recover();
    let kept = s.contents().versions(b"k").len();
    s.power_loss();
    s.recover();
    assert_eq!(
        s.contents().versions(b"k").len(),
        kept,
        "what survived a power loss was on the disk"
    );
}

#[test]
fn test_a_drop_interrupted_at_its_start_did_not_happen() {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"\x00a", 1), PersistMode::Sync).unwrap();
    s.inject(Fault::CrashDuringNextDrop(DropStage::Start));
    assert!(s.drop_prefixes(2, &[b"\x00".to_vec()], Vec::new()).is_err());
    s.recover();
    assert_eq!(s.contents().versions(b"\x00a").len(), 1);
}

#[test]
fn test_a_drop_whose_record_was_written_finishes_after_a_crash() {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"\x00a", 1), PersistMode::Sync).unwrap();
    s.inject(Fault::CrashDuringNextDrop(DropStage::RecordWritten));
    assert!(
        s.drop_prefixes(
            2,
            &[b"\x00".to_vec()],
            vec![Entry::put(b"\x00b".to_vec(), 1, 1, vec![])]
        )
        .is_err()
    );
    s.recover();
    let contents = s.contents();
    assert!(contents.versions(b"\x00a").is_empty());
    assert_eq!(contents.versions(b"\x00b").len(), 1);
}

#[test]
fn test_a_drop_whose_record_was_not_synced_survives_a_power_loss_whole_or_not_at_all() {
    let outcomes: BTreeSet<bool> = (0..16)
        .map(|seed| {
            let s = store(seed, FaultConfig::none());
            s.apply(batch(b"\x00a", 1), PersistMode::Sync).unwrap();
            s.inject(Fault::PowerLossDuringNextDrop(DropStage::RecordWritten));
            let entry = Entry::put(b"\x00b".to_vec(), 1, 1, vec![]);
            assert!(
                s.drop_prefixes(2, &[b"\x00".to_vec()], vec![entry])
                    .is_err()
            );
            s.recover();
            let contents = s.contents();
            let dropped = contents.versions(b"\x00a").is_empty();
            assert_eq!(contents.versions(b"\x00b").len(), usize::from(dropped));
            dropped
        })
        .collect();
    assert_eq!(outcomes.len(), 2);
}

#[test]
fn test_a_read_fails_when_forced() {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    s.inject(Fault::FailNextRead);
    let snapshot = s.snapshot(1).unwrap();
    let err = snapshot.get(b"k").unwrap_err();
    assert!(matches!(&err.0, StoreErrorEnum::ReadFailed { key, .. } if key == b"k"));
    assert!(
        snapshot.get(b"k").unwrap().is_some(),
        "only the next read fails"
    );
}

#[test]
fn test_a_scan_fails_partway() {
    let s = store(1, FaultConfig::none());
    (1..=5u8).for_each(|i| s.apply(batch(&[b'p', i], 1), PersistMode::Buffer).unwrap());
    s.inject(Fault::FailNextScanAfter(2));
    let items: Vec<_> = s
        .snapshot(1)
        .unwrap()
        .scan(&Scan::prefix(b"p".to_vec()))
        .collect();
    assert_eq!(items.len(), 3);
    assert!(items[..2].iter().all(Result::is_ok));
    assert!(
        matches!(&items[2], Err(StoreError(StoreErrorEnum::ReadFailed { key, .. })) if key == &vec![b'p', 3])
    );
}

#[test]
fn test_a_corrupted_value_reaches_the_decoder_or_is_refused() {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    s.corrupt(b"k", 1, Corruption::FlipValueByte);
    let item = s.get_at(b"k", 1).unwrap().expect("the bytes are returned");
    assert_ne!(
        item.value(),
        b"value",
        "the decoder above the store must refuse them"
    );
    assert_eq!(item.value().len(), 5);
    s.corrupt(b"k", 1, Corruption::Truncate);
    assert!(matches!(
        s.get_at(b"k", 1).unwrap_err().0,
        StoreErrorEnum::Corrupted { .. }
    ));
    s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    assert_eq!(
        s.get_at(b"k", 1).unwrap().unwrap().value(),
        b"value",
        "rewriting the version heals it"
    );
}

#[test]
fn test_drawn_faults_follow_the_configuration() {
    let s = store(
        5,
        FaultConfig::none()
            .with_read_failure(1000)
            .with_slow(1000, 7),
    );
    s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    let snapshot = s.snapshot(1).unwrap();
    assert!(snapshot.get(b"k").is_err());
    assert!(snapshot.get_many(&[b"k"]).is_err());
    assert!(s.clock().now() > 0, "slow operations advance the clock");
    let log = s.take_log();
    assert!(
        log.iter()
            .any(|r| matches!(r, OpRecord::Slowed { nanos } if *nanos <= 7))
    );
    assert!(s.take_log().is_empty(), "the log is taken once");
}

#[test]
fn test_drawn_batch_failures_poison_the_store() {
    let s = store(5, FaultConfig::none().with_batch_failure(1000));
    assert!(matches!(
        s.apply(batch(b"k", 1), PersistMode::Buffer),
        Err(StoreError(StoreErrorEnum::WriteFailed(_)))
    ));
    assert!(s.needs_recovery());
    let s = store(5, FaultConfig::none().with_crash_during_batch(1000));
    assert!(s.apply(batch(b"k", 1), PersistMode::Buffer).is_err());
    assert!(
        s.take_log()
            .iter()
            .any(|r| matches!(r, OpRecord::CrashedDuringBatch { .. }))
    );
}

#[test]
fn test_drawn_drop_faults_interrupt_drops() {
    let s = store(5, FaultConfig::none().with_crash_during_drop(1000));
    assert!(s.drop_all(Vec::new()).is_err());
    assert!(s.take_log().iter().any(|r| matches!(
        r,
        OpRecord::DropInterrupted {
            power_loss: false,
            ..
        }
    )));
    let s = store(5, FaultConfig::none().with_power_loss_during_drop(1000));
    assert!(s.drop_all(Vec::new()).is_err());
    assert!(s.take_log().iter().any(|r| matches!(
        r,
        OpRecord::DropInterrupted {
            power_loss: true,
            ..
        }
    )));
}

#[test]
fn test_drawn_corruptions_and_scan_failures_are_logged() {
    let s = store(
        5,
        FaultConfig::none()
            .with_corruption(1000)
            .with_scan_failure(1000),
    );
    s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    let snapshot = s.snapshot(1).unwrap();
    let _ = snapshot.get(b"k");
    let items: Vec<_> = snapshot.scan(&Scan::prefix(Vec::new())).collect();
    assert!(items.last().is_some_and(Result::is_err));
    assert!(
        s.take_log()
            .iter()
            .any(|r| matches!(r, OpRecord::Corrupted { .. }))
    );
}
