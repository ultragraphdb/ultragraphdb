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
            assert_eq!(
                s.apply(two, PersistMode::Buffer).unwrap_err().to_string(),
                "Write to the store failed: simulated process crash while the batch was written"
            );
            assert!(s.needs_recovery());
            assert_eq!(s.snapshot(2).err(), Some(StoreError::Poisoned()));
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
    assert_eq!(
        s.drop_prefixes(2, &[b"\x00".to_vec()], Vec::new()),
        Err(StoreError::WriteFailed(
            "simulated process crash during a drop".into()
        ))
    );
    s.recover();
    assert_eq!(s.contents().versions(b"\x00a").len(), 1);
}

#[test]
fn test_a_drop_stopped_at_each_stage_is_finished_or_undone_whole() {
    // Whether the drop happened after recovery, over 16 seeds, by stage, for a crash and for a power loss: before
    // its record it did not; with its record handed to the system it survives a crash and perhaps a power loss;
    // once its record is synced recovery finishes it. 130 keys take three removal batches.
    let cases = [
        (DropStage::Start, [false].as_slice(), [false].as_slice()),
        (DropStage::RecordWritten, &[true], &[false, true]),
        (DropStage::RecordSynced, &[true], &[true]),
        (DropStage::DataRemoved, &[true], &[true]),
        (DropStage::EntriesSynced, &[true], &[true]),
    ];
    for (stage, after_crash, after_power_loss) in cases {
        for (power_loss, expected) in [(false, after_crash), (true, after_power_loss)] {
            let outcomes: BTreeSet<bool> = (0..16)
                .map(|seed| {
                    let s = store(seed, FaultConfig::none());
                    let mut data = WriteBatch::new();
                    (0..130u8)
                        .for_each(|i| data.push(Entry::put(vec![0x00, b'a', i], 1, 0x04, vec![])));
                    data.push(Entry::put(b"\x01kept".to_vec(), 1, 0x04, vec![]));
                    s.apply(data, PersistMode::Sync).unwrap();
                    s.inject(if power_loss {
                        Fault::PowerLossDuringNextDrop(stage)
                    } else {
                        Fault::CrashDuringNextDrop(stage)
                    });
                    let entry = Entry::put(b"\x00b".to_vec(), 1, 1, b"initial".to_vec());
                    let stopped = s.drop_prefixes(2, &[b"\x00".to_vec()], vec![entry]);
                    let kind = if power_loss {
                        "power loss"
                    } else {
                        "process crash"
                    };
                    assert_eq!(
                        stopped,
                        Err(StoreError::WriteFailed(format!(
                            "simulated {kind} during a drop"
                        )))
                    );
                    assert!(s.needs_recovery());
                    assert!(s.take_log().contains(&OpRecord::DropInterrupted {
                        ts: 2,
                        prefixes: Some(vec![b"\x00".to_vec()]),
                        entries: vec![Entry::put(b"\x00b".to_vec(), 1, 1, b"initial".to_vec())],
                        stage,
                        power_loss,
                    }));
                    s.recover();
                    let keys: Vec<Vec<u8>> = s.contents().keys().keys().cloned().collect();
                    let dropped = keys == [b"\x00b".to_vec(), b"\x01kept".to_vec()];
                    assert!(
                        dropped || keys.len() == 131,
                        "{stage:?}: {} keys",
                        keys.len()
                    );
                    dropped
                })
                .collect();
            assert_eq!(
                outcomes,
                expected.iter().copied().collect(),
                "{stage:?}, power loss {power_loss}"
            );
        }
    }
}

#[test]
fn test_a_drop_whose_record_was_written_finishes_after_a_crash() {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"\x00a", 1), PersistMode::Sync).unwrap();
    s.inject(Fault::CrashDuringNextDrop(DropStage::RecordWritten));
    assert_eq!(
        s.drop_prefixes(
            2,
            &[b"\x00".to_vec()],
            vec![Entry::put(b"\x00b".to_vec(), 1, 1, vec![])]
        ),
        Err(StoreError::WriteFailed(
            "simulated process crash during a drop".into()
        ))
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
            assert_eq!(
                s.drop_prefixes(2, &[b"\x00".to_vec()], vec![entry]),
                Err(StoreError::WriteFailed(
                    "simulated power loss during a drop".into()
                ))
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
    assert_eq!(
        snapshot.get(b"k"),
        Err(StoreError::ReadFailed(
            b"k".to_vec(),
            "simulated read failure".into()
        ))
    );
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
    let keys: Vec<Vec<u8>> = items[..2]
        .iter()
        .map(|i| i.as_ref().unwrap().key().to_vec())
        .collect();
    assert_eq!(keys, [vec![b'p', 1], vec![b'p', 2]]);
    assert_eq!(
        items[2..],
        [Err(StoreError::ReadFailed(
            vec![b'p', 3],
            "simulated read failure partway through a scan".into()
        ))]
    );
}

#[test]
fn test_a_scan_failing_at_its_ends_names_the_key_it_stopped_at() {
    // After none of the items it fails at the first key; after all of them, or more, at the start of its range.
    for (after, key) in [(0, vec![b'p', 1]), (3, b"p".to_vec()), (9, b"p".to_vec())] {
        let s = store(1, FaultConfig::none());
        (1..=3u8).for_each(|i| s.apply(batch(&[b'p', i], 1), PersistMode::Buffer).unwrap());
        s.inject(Fault::FailNextScanAfter(after));
        let items: Vec<_> = s
            .snapshot(1)
            .unwrap()
            .scan(&Scan::prefix(b"p".to_vec()))
            .collect();
        assert_eq!(items.len(), after.min(3) + 1, "after {after}");
        assert_eq!(
            items.last(),
            Some(&Err(StoreError::ReadFailed(
                key,
                "simulated read failure partway through a scan".into()
            )))
        );
    }
}

#[test]
fn test_a_scan_stopped_by_a_corrupted_version_yields_no_second_error() {
    // A scan yields its error and then ends: a forced failure past the corrupted version the scan already stopped
    // at adds nothing after it. One before it fails the scan there instead.
    let s = store(1, FaultConfig::none());
    (1..=5u8).for_each(|i| s.apply(batch(&[b'p', i], 1), PersistMode::Buffer).unwrap());
    s.corrupt(&[b'p', 3], 1, Corruption::Truncate);
    s.inject(Fault::FailNextScanAfter(1));
    let items: Vec<_> = s
        .snapshot(1)
        .unwrap()
        .scan(&Scan::prefix(b"p".to_vec()))
        .collect();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[1],
        Err(StoreError::ReadFailed(
            vec![b'p', 2],
            "simulated read failure partway through a scan".into()
        ))
    );
    for after in [3, 9] {
        let s = store(1, FaultConfig::none());
        (1..=5u8).for_each(|i| s.apply(batch(&[b'p', i], 1), PersistMode::Buffer).unwrap());
        s.corrupt(&[b'p', 3], 1, Corruption::Truncate);
        s.inject(Fault::FailNextScanAfter(after));
        let items: Vec<_> = s
            .snapshot(1)
            .unwrap()
            .scan(&Scan::prefix(b"p".to_vec()))
            .collect();
        assert_eq!(
            items.iter().filter(|i| i.is_err()).count(),
            1,
            "after {after}: {items:?}"
        );
        assert_eq!(items.len(), 3, "after {after}");
        assert!(matches!(
            items.last(),
            Some(Err(StoreError(StoreErrorEnum::Corrupted { key, .. }))) if *key == [b'p', 3]
        ));
    }
}

#[test]
fn test_a_corrupted_value_reaches_the_decoder_or_is_refused() {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    s.corrupt(b"k", 1, Corruption::FlipValueByte);
    let item = s.get_at(b"k", 1).unwrap().expect("the bytes are returned");
    // The middle byte of the five past the two-byte prefix, 'l' (0x6c), inverted; the decoder above must refuse it.
    assert_eq!(item.value(), b"va\x93ue");
    assert_eq!((item.meta(), item.version()), (0x04, 1));
    s.corrupt(b"k", 1, Corruption::Truncate);
    let refused = s.get_at(b"k", 1).unwrap_err();
    assert_eq!(
        refused,
        StoreError::Corrupted(
            b"k".to_vec(),
            "value too short to hold the version prefix".into()
        )
    );
    assert_eq!(
        refused.to_string(),
        "Corrupted value of key 6b: value too short to hold the version prefix"
    );
    s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    assert_eq!(
        s.get_at(b"k", 1).unwrap().unwrap().value(),
        b"value",
        "rewriting the version heals it"
    );
}

#[test]
fn test_flipping_a_value_too_short_to_flip_truncates_it() {
    let s = store(1, FaultConfig::none());
    let mut b = WriteBatch::new();
    b.push(Entry::put(b"k".to_vec(), 1, 0x04, Vec::new()));
    s.apply(b, PersistMode::Buffer).unwrap();
    s.corrupt(b"k", 1, Corruption::FlipValueByte);
    assert_eq!(
        s.get_at(b"k", 1),
        Err(StoreError::Corrupted(
            b"k".to_vec(),
            "value too short to hold the version prefix".into()
        ))
    );
    assert_eq!(s.contents().versions(b"k")[0].value(), [0x04]);
}

#[test]
fn test_a_corrupted_version_breaks_only_its_own_reads() {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"k", 1), PersistMode::Buffer).unwrap();
    s.apply(batch(b"k", 2), PersistMode::Buffer).unwrap();
    s.corrupt(b"k", 1, Corruption::Truncate);
    assert_eq!(s.get_at(b"k", 2).unwrap().unwrap().value(), b"value");
    assert_eq!(
        s.get_at(b"k", 1),
        Err(StoreError::Corrupted(
            b"k".to_vec(),
            "value too short to hold the version prefix".into()
        ))
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
    let failure = Err(StoreError::ReadFailed(
        b"k".to_vec(),
        "simulated read failure".into(),
    ));
    assert_eq!(snapshot.get(b"k"), failure);
    assert_eq!(snapshot.get_many(&[b"k"]), failure.map(|i| vec![i]));
    let log = s.take_log();
    let slowed: Vec<u64> = log
        .iter()
        .filter_map(|r| match r {
            OpRecord::Slowed { nanos } => Some(*nanos),
            _ => None,
        })
        .collect();
    assert_eq!(slowed.len(), 4, "every operation is slow: {log:?}");
    assert!(slowed.iter().all(|n| (1..=7).contains(n)), "{slowed:?}");
    assert_eq!(
        s.clock().now(),
        slowed.iter().sum::<u64>(),
        "slow operations advance the clock by their delays"
    );
    assert!(s.take_log().is_empty(), "the log is taken once");
}

#[test]
fn test_drawn_batch_failures_poison_the_store() {
    let s = store(5, FaultConfig::none().with_batch_failure(1000));
    assert_eq!(
        s.apply(batch(b"k", 1), PersistMode::Buffer),
        Err(StoreError::WriteFailed(
            "simulated failure during the batch's persist".into()
        ))
    );
    assert!(s.needs_recovery());
    let s = store(5, FaultConfig::none().with_crash_during_batch(1000));
    assert_eq!(
        s.apply(batch(b"k", 1), PersistMode::Buffer),
        Err(StoreError::WriteFailed(
            "simulated process crash while the batch was written".into()
        ))
    );
    assert!(s.take_log().contains(&OpRecord::CrashedDuringBatch {
        batch: batch(b"k", 1)
    }));
}

#[test]
fn test_drawn_drop_faults_interrupt_drops() {
    let s = store(5, FaultConfig::none().with_crash_during_drop(1000));
    assert_eq!(
        s.drop_all(Vec::new()),
        Err(StoreError::WriteFailed(
            "simulated process crash during a drop".into()
        ))
    );
    // The stage is drawn; what was dropped is not.
    assert!(s.take_log().iter().any(|r| matches!(
        r,
        OpRecord::DropInterrupted {
            ts: 0,
            prefixes: None,
            entries,
            power_loss: false,
            ..
        } if entries.is_empty()
    )));
    let s = store(5, FaultConfig::none().with_power_loss_during_drop(1000));
    assert_eq!(
        s.drop_all(Vec::new()),
        Err(StoreError::WriteFailed(
            "simulated power loss during a drop".into()
        ))
    );
    assert!(s.take_log().iter().any(|r| matches!(
        r,
        OpRecord::DropInterrupted {
            ts: 0,
            prefixes: None,
            entries,
            power_loss: true,
            ..
        } if entries.is_empty()
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
    let read = snapshot.get(b"k");
    let items: Vec<_> = snapshot.scan(&Scan::prefix(Vec::new())).collect();
    assert_eq!(items.iter().filter(|i| i.is_err()).count(), 1, "{items:?}");
    assert!(matches!(
        items.last(),
        Some(Err(StoreError(
            StoreErrorEnum::ReadFailed { .. } | StoreErrorEnum::Corrupted { .. }
        )))
    ));
    let log = s.take_log();
    let corrupted: Vec<&OpRecord> = log
        .iter()
        .filter(|r| matches!(r, OpRecord::Corrupted { .. }))
        .collect();
    assert_eq!(
        corrupted.len(),
        2,
        "the read and the scan each corrupt: {log:?}"
    );
    assert!(corrupted.iter().all(|r| matches!(
        r,
        OpRecord::Corrupted { key, version: 1, .. } if key == b"k"
    )));
    assert!(log.contains(&OpRecord::Read {
        id: 0,
        key: b"k".to_vec(),
        result: read
    }));
}

/// Batch `version` of key `key` with the versionless `writes` beside it.
fn with_versionless(key: &[u8], version: u64, writes: &[(&[u8], &[u8])]) -> WriteBatch {
    let mut batch = batch(key, version);
    writes
        .iter()
        .for_each(|(k, v)| batch.put_versionless(k.to_vec(), v.to_vec()));
    batch
}

/// A failed batch or a crash during one keeps its versionless writes with its entries after recovery, or neither:
/// before recovery a poisoned store reads without the failed batch, and a store whose process is down reads nothing.
#[test]
fn test_a_batch_in_flight_keeps_its_versionless_writes_with_its_entries() {
    for fault in [Fault::FailNextBatch, Fault::CrashDuringNextBatch] {
        let outcomes: BTreeSet<bool> = (0..16)
            .map(|seed| {
                let s = store(seed, FaultConfig::none());
                s.apply(
                    with_versionless(b"a", 1, &[(b"k", b"1")]),
                    PersistMode::Buffer,
                )
                .unwrap();
                s.inject(fault);
                let (refusal, before) = if fault == Fault::FailNextBatch {
                    (
                        "simulated failure during the batch's persist",
                        Ok(Some(b"1".to_vec())),
                    )
                } else {
                    (
                        "simulated process crash while the batch was written",
                        Err(StoreError::Poisoned()),
                    )
                };
                assert_eq!(
                    s.apply(
                        with_versionless(b"b", 2, &[(b"k", b"2"), (b"j", b"3")]),
                        PersistMode::Buffer
                    ),
                    Err(StoreError::WriteFailed(refusal.into()))
                );
                assert_eq!(s.get_versionless(b"k"), before, "{fault:?}");
                s.recover();
                let contents = s.contents();
                let kept = !contents.versions(b"b").is_empty();
                // Hand-derived: batch 2 whole writes k = "2" and j = "3"; lost, k keeps batch 1's "1".
                let expected: Vec<(&[u8], &[u8])> = if kept {
                    vec![(b"j", b"3"), (b"k", b"2")]
                } else {
                    vec![(b"k", b"1")]
                };
                assert_eq!(
                    contents
                        .versionless()
                        .iter()
                        .map(|(k, v)| (k.as_slice(), v.as_slice()))
                        .collect::<Vec<_>>(),
                    expected,
                    "{fault:?} seed {seed}"
                );
                kept
            })
            .collect();
        assert_eq!(outcomes.len(), 2, "{fault:?}: both outcomes occur");
    }
}

/// A forced read failure reaches a versionless read through the store and through a snapshot, names the key, and
/// fails that read alone.
#[test]
fn test_a_versionless_read_fails_when_forced() {
    let s = store(1, FaultConfig::none());
    s.apply(
        with_versionless(b"a", 1, &[(b"k", b"1")]),
        PersistMode::Buffer,
    )
    .unwrap();
    let snapshot = s.snapshot(1).unwrap();
    let refused = Err(StoreError::ReadFailed(
        b"k".to_vec(),
        "simulated read failure".into(),
    ));
    s.inject(Fault::FailNextRead);
    assert_eq!(s.get_versionless(b"k"), refused);
    assert_eq!(s.get_versionless(b"k"), Ok(Some(b"1".to_vec())));
    s.inject(Fault::FailNextRead);
    assert_eq!(snapshot.get_versionless(b"k"), refused);
    assert_eq!(snapshot.get_versionless(b"k"), Ok(Some(b"1".to_vec())));
    let failures = s
        .take_log()
        .into_iter()
        .filter(|r| matches!(r, OpRecord::ReadVersionless { result: Err(_), .. }))
        .count();
    assert_eq!(failures, 2, "each failure is logged");
}

/// A drop of everything stopped by a power loss clears the keyspace without versions exactly when its record
/// survives: never when stopped at its start, always once its record was synced, and after a crash once its record
/// was written.
#[test]
fn test_an_interrupted_drop_of_everything_clears_the_versionless_keys_with_its_record() {
    let power_loss = "simulated power loss during a drop";
    let crash = "simulated process crash during a drop";
    let cases = [
        (
            Fault::PowerLossDuringNextDrop(DropStage::Start),
            power_loss,
            false,
        ),
        (
            Fault::CrashDuringNextDrop(DropStage::RecordWritten),
            crash,
            true,
        ),
        (
            Fault::PowerLossDuringNextDrop(DropStage::RecordSynced),
            power_loss,
            true,
        ),
        (
            Fault::PowerLossDuringNextDrop(DropStage::DataRemoved),
            power_loss,
            true,
        ),
        (
            Fault::PowerLossDuringNextDrop(DropStage::EntriesSynced),
            power_loss,
            true,
        ),
    ];
    for (fault, refusal, cleared) in cases {
        for seed in 0..8 {
            let s = store(seed, FaultConfig::none());
            s.apply(
                with_versionless(b"a", 1, &[(b"k", b"1")]),
                PersistMode::Sync,
            )
            .unwrap();
            s.inject(fault);
            assert_eq!(
                s.drop_all(Vec::new()),
                Err(StoreError::WriteFailed(refusal.into())),
                "{fault:?}"
            );
            s.recover();
            assert_eq!(
                s.contents().versionless().is_empty(),
                cleared,
                "{fault:?} seed {seed}"
            );
            assert_eq!(
                s.get_versionless(b"k"),
                Ok((!cleared).then(|| b"1".to_vec()))
            );
        }
    }
}
