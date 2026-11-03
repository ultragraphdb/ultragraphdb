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
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

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
fn test_a_crash_keeps_every_batch_handed_to_the_system() -> TestResult {
    let s = store(1, FaultConfig::none());
    for v in 1..=5 {
        s.apply(batch(b"k", v), PersistMode::Buffer)?;
    }
    s.crash();
    assert!(s.needs_recovery());
    assert_eq!(
        s.apply(batch(b"k", 6), PersistMode::Buffer),
        Err(StoreError::poisoned())
    );
    assert_eq!(
        s.snapshot(1).err(),
        Some(StoreError::poisoned()),
        "a process that is down serves no read"
    );
    s.recover();
    assert_eq!(s.applied_ts(), 5);
    assert_eq!(s.contents().versions(b"k").len(), 5);
    Ok(())
}

#[test]
fn test_a_crash_during_a_batch_keeps_it_whole_or_not_at_all() -> TestResult {
    let outcomes: BTreeSet<usize> = (0..16)
        .map(|seed| -> TestResult<_> {
            let s = store(seed, FaultConfig::none());
            s.apply(batch(b"a", 1), PersistMode::Buffer)?;
            s.inject(Fault::CrashDuringNextBatch);
            let mut two = batch(b"b", 2);
            two.push(Entry::put(b"c".to_vec(), 2, 0x04, vec![]));
            assert_eq!(
                s.apply(two, PersistMode::Buffer)
                    .err()
                    .context("an error")?
                    .to_string(),
                "Write to the store failed: simulated process crash while the batch was written"
            );
            assert!(s.needs_recovery());
            assert_eq!(s.snapshot(2).err(), Some(StoreError::poisoned()));
            s.recover();
            let held = [&b"b"[..], b"c"]
                .iter()
                .filter(|k| !s.contents().versions(k).is_empty())
                .count();
            assert!(held == 0 || held == 2);
            assert_eq!(s.contents().versions(b"a").len(), 1);
            Ok(held)
        })
        .collect::<TestResult<_>>()?;
    assert_eq!(outcomes.len(), 2);
    Ok(())
}

#[test]
fn test_a_power_loss_keeps_the_synced_batches_and_a_prefix_of_the_rest() -> TestResult {
    let survivors: BTreeSet<u64> = (0..24)
        .map(|seed| -> TestResult<_> {
            let s = store(seed, FaultConfig::none());
            for v in 1..=3 {
                s.apply(batch(b"k", v), PersistMode::Buffer)?;
            }
            s.sync()?;
            for v in 4..=8 {
                s.apply(batch(b"k", v), PersistMode::Buffer)?;
            }
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
            Ok(kept)
        })
        .collect::<TestResult<_>>()?;
    assert!(survivors.len() >= 4, "{survivors:?}");
    Ok(())
}

#[test]
fn test_a_power_loss_after_recovery_keeps_what_survived_the_first() -> TestResult {
    let s = store(3, FaultConfig::none());
    for v in 1..=4 {
        s.apply(batch(b"k", v), PersistMode::Buffer)?;
    }
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
    Ok(())
}

#[test]
fn test_a_drop_interrupted_at_its_start_did_not_happen() -> TestResult {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"\x00a", 1), PersistMode::Sync)?;
    s.inject(Fault::CrashDuringNextDrop(DropStage::Start));
    assert_eq!(
        s.drop_prefixes(2, &[b"\x00".to_vec()], Vec::new()),
        Err(StoreError::write_failed(
            "simulated process crash during a drop".into()
        ))
    );
    s.recover();
    assert_eq!(s.contents().versions(b"\x00a").len(), 1);
    Ok(())
}

#[test]
fn test_a_drop_stopped_at_each_stage_is_finished_or_undone_whole() -> TestResult {
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
                .map(|seed| -> TestResult<_> {
                    let s = store(seed, FaultConfig::none());
                    let mut data = WriteBatch::new();
                    (0..130u8)
                        .for_each(|i| data.push(Entry::put(vec![0x00, b'a', i], 1, 0x04, vec![])));
                    data.push(Entry::put(b"\x01kept".to_vec(), 1, 0x04, vec![]));
                    s.apply(data, PersistMode::Sync)?;
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
                        Err(StoreError::write_failed(format!(
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
                    Ok(dropped)
                })
                .collect::<TestResult<_>>()?;
            assert_eq!(
                outcomes,
                expected.iter().copied().collect(),
                "{stage:?}, power loss {power_loss}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_a_drop_whose_record_was_written_finishes_after_a_crash() -> TestResult {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"\x00a", 1), PersistMode::Sync)?;
    s.inject(Fault::CrashDuringNextDrop(DropStage::RecordWritten));
    assert_eq!(
        s.drop_prefixes(
            2,
            &[b"\x00".to_vec()],
            vec![Entry::put(b"\x00b".to_vec(), 1, 1, vec![])]
        ),
        Err(StoreError::write_failed(
            "simulated process crash during a drop".into()
        ))
    );
    s.recover();
    let contents = s.contents();
    assert!(contents.versions(b"\x00a").is_empty());
    assert_eq!(contents.versions(b"\x00b").len(), 1);
    Ok(())
}

#[test]
fn test_a_drop_whose_record_was_not_synced_survives_a_power_loss_whole_or_not_at_all() -> TestResult
{
    let outcomes: BTreeSet<bool> = (0..16)
        .map(|seed| -> TestResult<_> {
            let s = store(seed, FaultConfig::none());
            s.apply(batch(b"\x00a", 1), PersistMode::Sync)?;
            s.inject(Fault::PowerLossDuringNextDrop(DropStage::RecordWritten));
            let entry = Entry::put(b"\x00b".to_vec(), 1, 1, vec![]);
            assert_eq!(
                s.drop_prefixes(2, &[b"\x00".to_vec()], vec![entry]),
                Err(StoreError::write_failed(
                    "simulated power loss during a drop".into()
                ))
            );
            s.recover();
            let contents = s.contents();
            let dropped = contents.versions(b"\x00a").is_empty();
            assert_eq!(contents.versions(b"\x00b").len(), usize::from(dropped));
            Ok(dropped)
        })
        .collect::<TestResult<_>>()?;
    assert_eq!(outcomes.len(), 2);
    Ok(())
}

#[test]
fn test_a_read_fails_when_forced() -> TestResult {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"k", 1), PersistMode::Buffer)?;
    s.inject(Fault::FailNextRead);
    let snapshot = s.snapshot(1)?;
    assert_eq!(
        snapshot.get(b"k"),
        Err(StoreError::read_failed(
            b"k".to_vec(),
            "simulated read failure".into()
        ))
    );
    assert!(snapshot.get(b"k")?.is_some(), "only the next read fails");
    Ok(())
}

#[test]
fn test_unfaulted_reads_fail_no_drawn_read() -> TestResult {
    let s = store(1, FaultConfig::none().with_read_failure(1000));
    s.apply(batch(b"k", 1), PersistMode::Buffer)?;
    let snapshot = s.snapshot(1)?;
    let held = s.unfaulted_reads(|| -> TestResult<_> {
        Ok((
            snapshot.get(b"k")?.map(Item::into_value),
            s.get_versionless(b"k")?,
        ))
    })?;
    assert_eq!(held, (Some(b"value".to_vec()), None));
    assert!(
        matches!(
            snapshot.get(b"k"),
            Err(StoreError(StoreErrorEnum::ReadFailed { .. }))
        ),
        "reads fail again after"
    );
    Ok(())
}

#[test]
fn test_unfaulted_reads_keep_a_forced_read_fault_for_the_read_after() -> TestResult {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"k", 1), PersistMode::Buffer)?;
    s.inject(Fault::FailNextRead);
    let snapshot = s.snapshot(1)?;
    let held = s.unfaulted_reads(|| snapshot.get(b"k"))?;
    assert_eq!(held.map(Item::into_value), Some(b"value".to_vec()));
    assert_eq!(
        snapshot.get(b"k"),
        Err(StoreError::read_failed(
            b"k".to_vec(),
            "simulated read failure".into()
        ))
    );
    assert!(snapshot.get(b"k")?.is_some(), "only the forced read fails");
    Ok(())
}

#[test]
fn test_unfaulted_reads_fail_no_scan() -> TestResult {
    let s = store(1, FaultConfig::none().with_scan_failure(1000));
    for i in 1..=3u8 {
        s.apply(batch(&[b'p', i], 1), PersistMode::Buffer)?;
    }
    s.inject(Fault::FailNextScanAfter(1));
    let scan = Scan::prefix(b"p".to_vec());
    let held: Vec<_> =
        s.unfaulted_reads(|| -> TestResult<_> { Ok(s.snapshot(1)?.scan(&scan).collect()) })?;
    assert_eq!(held.len(), 3);
    assert!(held.iter().all(Result::is_ok), "{held:?}");
    let after: Vec<_> = s.snapshot(1)?.scan(&scan).collect();
    assert_eq!(
        after.len(),
        2,
        "the forced scan fault waits for the first scan after"
    );
    Ok(())
}

#[test]
fn test_a_scan_fails_partway() -> TestResult {
    let s = store(1, FaultConfig::none());
    for i in 1..=5u8 {
        s.apply(batch(&[b'p', i], 1), PersistMode::Buffer)?;
    }
    s.inject(Fault::FailNextScanAfter(2));
    let items: Vec<_> = s.snapshot(1)?.scan(&Scan::prefix(b"p".to_vec())).collect();
    let keys: Vec<Vec<u8>> = items[..2]
        .iter()
        .map(|i| -> TestResult<_> { Ok(i.as_ref()?.key().to_vec()) })
        .collect::<TestResult<_>>()?;
    assert_eq!(keys, [vec![b'p', 1], vec![b'p', 2]]);
    assert_eq!(
        items[2..],
        [Err(StoreError::read_failed(
            vec![b'p', 3],
            "simulated read failure partway through a scan".into()
        ))]
    );
    Ok(())
}

#[test]
fn test_a_scan_failing_at_its_ends_names_the_key_it_stopped_at() -> TestResult {
    // After none of the items it fails at the first key; after all of them, or more, at the start of its range.
    for (after, key) in [(0, vec![b'p', 1]), (3, b"p".to_vec()), (9, b"p".to_vec())] {
        let s = store(1, FaultConfig::none());
        for i in 1..=3u8 {
            s.apply(batch(&[b'p', i], 1), PersistMode::Buffer)?;
        }
        s.inject(Fault::FailNextScanAfter(after));
        let items: Vec<_> = s.snapshot(1)?.scan(&Scan::prefix(b"p".to_vec())).collect();
        assert_eq!(items.len(), after.min(3) + 1, "after {after}");
        assert_eq!(
            items.last(),
            Some(&Err(StoreError::read_failed(
                key,
                "simulated read failure partway through a scan".into()
            )))
        );
    }
    Ok(())
}

#[test]
fn test_a_scan_stopped_by_a_corrupted_version_yields_no_second_error() -> TestResult {
    // A scan yields its error and then ends: a forced failure past the corrupted version the scan already stopped
    // at adds nothing after it. One before it fails the scan there instead.
    let s = store(1, FaultConfig::none());
    for i in 1..=5u8 {
        s.apply(batch(&[b'p', i], 1), PersistMode::Buffer)?;
    }
    s.corrupt(&[b'p', 3], 1, Corruption::Truncate);
    s.inject(Fault::FailNextScanAfter(1));
    let items: Vec<_> = s.snapshot(1)?.scan(&Scan::prefix(b"p".to_vec())).collect();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[1],
        Err(StoreError::read_failed(
            vec![b'p', 2],
            "simulated read failure partway through a scan".into()
        ))
    );
    for after in [3, 9] {
        let fresh = store(1, FaultConfig::none());
        for i in 1..=5u8 {
            fresh.apply(batch(&[b'p', i], 1), PersistMode::Buffer)?;
        }
        fresh.corrupt(&[b'p', 3], 1, Corruption::Truncate);
        fresh.inject(Fault::FailNextScanAfter(after));
        let fresh_items: Vec<_> = fresh
            .snapshot(1)?
            .scan(&Scan::prefix(b"p".to_vec()))
            .collect();
        assert_eq!(
            fresh_items.iter().filter(|i| i.is_err()).count(),
            1,
            "after {after}: {fresh_items:?}"
        );
        assert_eq!(fresh_items.len(), 3, "after {after}");
        assert!(matches!(
            fresh_items.last(),
            Some(Err(StoreError(StoreErrorEnum::Corrupted { key, .. }))) if *key == [b'p', 3]
        ));
    }
    Ok(())
}

#[test]
fn test_a_corrupted_value_reaches_the_decoder_or_is_refused() -> TestResult {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"k", 1), PersistMode::Buffer)?;
    s.corrupt(b"k", 1, Corruption::FlipValueByte);
    let item = s.get_at(b"k", 1)?.context("the bytes are returned")?;
    // The middle byte of the five past the two-byte prefix, 'l' (0x6c), inverted; the decoder above must refuse it.
    assert_eq!(item.value(), b"va\x93ue");
    assert_eq!((item.meta(), item.version()), (0x04, 1));
    s.corrupt(b"k", 1, Corruption::Truncate);
    let refused = s.get_at(b"k", 1).err().context("an error")?;
    assert_eq!(
        refused,
        StoreError::corrupted(
            b"k".to_vec(),
            "value too short to hold the version prefix".into()
        )
    );
    assert_eq!(
        refused.to_string(),
        "Corrupted value of key 6b: value too short to hold the version prefix"
    );
    s.apply(batch(b"k", 1), PersistMode::Buffer)?;
    assert_eq!(
        s.get_at(b"k", 1)?.context("a value")?.value(),
        b"value",
        "rewriting the version heals it"
    );
    Ok(())
}

#[test]
fn test_flipping_a_value_too_short_to_flip_truncates_it() -> TestResult {
    let s = store(1, FaultConfig::none());
    let mut b = WriteBatch::new();
    b.push(Entry::put(b"k".to_vec(), 1, 0x04, Vec::new()));
    s.apply(b, PersistMode::Buffer)?;
    s.corrupt(b"k", 1, Corruption::FlipValueByte);
    assert_eq!(
        s.get_at(b"k", 1),
        Err(StoreError::corrupted(
            b"k".to_vec(),
            "value too short to hold the version prefix".into()
        ))
    );
    assert_eq!(s.contents().versions(b"k")[0].value(), [0x04]);
    Ok(())
}

#[test]
fn test_a_corrupted_version_breaks_only_its_own_reads() -> TestResult {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"k", 1), PersistMode::Buffer)?;
    s.apply(batch(b"k", 2), PersistMode::Buffer)?;
    s.corrupt(b"k", 1, Corruption::Truncate);
    assert_eq!(s.get_at(b"k", 2)?.context("a value")?.value(), b"value");
    assert_eq!(
        s.get_at(b"k", 1),
        Err(StoreError::corrupted(
            b"k".to_vec(),
            "value too short to hold the version prefix".into()
        ))
    );
    Ok(())
}

/// A drop of everything heals every corruption: the version its entries write again at the same key and version reads
/// as written, though the drop wrote it with no batch of its own.
#[test]
fn test_a_drop_of_everything_heals_every_corruption() -> TestResult {
    let s = store(1, FaultConfig::none());
    s.apply(batch(b"k", 1), PersistMode::Buffer)?;
    s.corrupt(b"k", 1, Corruption::Truncate);
    assert!(s.get_at(b"k", 1).is_err(), "corrupted before the drop");
    s.drop_all(vec![Entry::put(b"k".to_vec(), 1, 0x04, b"value".to_vec())])?;
    assert_eq!(
        s.get_at(b"k", 1)?.context("the drop's entry")?.value(),
        b"value"
    );
    Ok(())
}

#[test]
fn test_drawn_faults_follow_the_configuration() -> TestResult {
    let s = store(
        5,
        FaultConfig::none()
            .with_read_failure(1000)
            .with_slow(1000, 7),
    );
    s.apply(batch(b"k", 1), PersistMode::Buffer)?;
    let snapshot = s.snapshot(1)?;
    let failure = Err(StoreError::read_failed(
        b"k".to_vec(),
        "simulated read failure".into(),
    ));
    assert_eq!(snapshot.get(b"k"), failure);
    assert_eq!(snapshot.get_many(&[b"k"]), failure.map(|i| vec![i]));
    let log = s.take_log();
    let slowed: Vec<u64> = log
        .iter()
        .filter_map(|r| {
            if let OpRecord::Slowed { nanos } = r {
                Some(*nanos)
            } else {
                None
            }
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
    Ok(())
}

#[test]
fn test_drawn_batch_failures_poison_the_store() {
    let s = store(5, FaultConfig::none().with_batch_failure(1000));
    assert_eq!(
        s.apply(batch(b"k", 1), PersistMode::Buffer),
        Err(StoreError::write_failed(
            "simulated failure during the batch's persist".into()
        ))
    );
    assert!(s.needs_recovery());
    let crashing = store(5, FaultConfig::none().with_crash_during_batch(1000));
    assert_eq!(
        crashing.apply(batch(b"k", 1), PersistMode::Buffer),
        Err(StoreError::write_failed(
            "simulated process crash while the batch was written".into()
        ))
    );
    assert!(crashing.take_log().contains(&OpRecord::CrashedDuringBatch {
        batch: batch(b"k", 1)
    }));
}

#[test]
fn test_drawn_drop_faults_interrupt_drops() {
    let s = store(5, FaultConfig::none().with_crash_during_drop(1000));
    assert_eq!(
        s.drop_all(Vec::new()),
        Err(StoreError::write_failed(
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
    let losing_power = store(5, FaultConfig::none().with_power_loss_during_drop(1000));
    assert_eq!(
        losing_power.drop_all(Vec::new()),
        Err(StoreError::write_failed(
            "simulated power loss during a drop".into()
        ))
    );
    assert!(losing_power.take_log().iter().any(|r| matches!(
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
fn test_drawn_corruptions_and_scan_failures_are_logged() -> TestResult {
    let s = store(
        5,
        FaultConfig::none()
            .with_corruption(1000)
            .with_scan_failure(1000),
    );
    s.apply(batch(b"k", 1), PersistMode::Buffer)?;
    let snapshot = s.snapshot(1)?;
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
    Ok(())
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
fn test_a_batch_in_flight_keeps_its_versionless_writes_with_its_entries() -> TestResult {
    for fault in [Fault::FailNextBatch, Fault::CrashDuringNextBatch] {
        let outcomes: BTreeSet<bool> = (0..16)
            .map(|seed| -> TestResult<_> {
                let s = store(seed, FaultConfig::none());
                s.apply(
                    with_versionless(b"a", 1, &[(b"k", b"1")]),
                    PersistMode::Buffer,
                )?;
                s.inject(fault);
                let (refusal, before) = if fault == Fault::FailNextBatch {
                    (
                        "simulated failure during the batch's persist",
                        Ok(Some(b"1".to_vec())),
                    )
                } else {
                    (
                        "simulated process crash while the batch was written",
                        Err(StoreError::poisoned()),
                    )
                };
                assert_eq!(
                    s.apply(
                        with_versionless(b"b", 2, &[(b"k", b"2"), (b"j", b"3")]),
                        PersistMode::Buffer
                    ),
                    Err(StoreError::write_failed(refusal.into()))
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
                Ok(kept)
            })
            .collect::<TestResult<_>>()?;
        assert_eq!(outcomes.len(), 2, "{fault:?}: both outcomes occur");
    }
    Ok(())
}

/// A forced read failure reaches a versionless read through the store and through a snapshot, names the key, and
/// fails that read alone.
#[test]
fn test_a_versionless_read_fails_when_forced() -> TestResult {
    let s = store(1, FaultConfig::none());
    s.apply(
        with_versionless(b"a", 1, &[(b"k", b"1")]),
        PersistMode::Buffer,
    )?;
    let snapshot = s.snapshot(1)?;
    let refused = Err(StoreError::read_failed(
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
    Ok(())
}

/// A drop of everything stopped by a power loss clears the keyspace without versions exactly when its record
/// survives: never when stopped at its start, always once its record was synced, and after a crash once its record
/// was written.
#[test]
fn test_an_interrupted_drop_of_everything_clears_the_versionless_keys_with_its_record() -> TestResult
{
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
            )?;
            s.inject(fault);
            assert_eq!(
                s.drop_all(Vec::new()),
                Err(StoreError::write_failed(refusal.into())),
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
    Ok(())
}
