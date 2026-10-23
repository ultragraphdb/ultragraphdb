/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use cucumber::{World, then, when};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_sim::{
    Contents, Corruption, DropStage, Event, Fault, FaultConfig, Invariant, RunOutcome, Seed,
    SimClock, SimStore, Simulation, StoreWorkload, Violation,
};
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

/// The key read, the point read's result and the scan's items.
type CorruptedRead = (
    Vec<u8>,
    Result<Option<Item>, StoreError>,
    Vec<Result<Item, StoreError>>,
);

#[derive(Debug, Default, World)]
struct Steps {
    runs: Vec<Result<RunOutcome, Violation>>,
    /// Per seed: the store after recovery and the batches synced before the power loss.
    recovered: Vec<(Contents, u64)>,
    /// Per seed: the refusal of the write after the failure, and the store after reopening.
    poisoned: Vec<(Result<(), StoreError>, Contents)>,
    read: Option<CorruptedRead>,
    violation: Option<Result<RunOutcome, Violation>>,
}

const BATCHES: u64 = 12;
const KEYS_PER_BATCH: u64 = 4;

/// Key `j` of batch `i`: a data key of namespace 0.
fn batch_key(i: u64, j: u64) -> Vec<u8> {
    let mut key = vec![0x00];
    key.extend_from_slice(&0u64.to_be_bytes());
    key.extend_from_slice(b"\x00\x04name");
    key.extend_from_slice(&(i * 100 + j).to_be_bytes());
    key
}

/// Batch `i`: its keys at version `i`, recording `i` as applied.
fn batch(i: u64) -> WriteBatch {
    let mut batch = WriteBatch::new();
    (0..KEYS_PER_BATCH)
        .for_each(|j| batch.push(Entry::put(batch_key(i, j), i, 0x04, vec![i as u8, j as u8])));
    batch.set_applied_ts(i);
    batch
}

/// How many keys of batch `i` the store holds at version `i`.
fn held(contents: &Contents, i: u64) -> u64 {
    (0..KEYS_PER_BATCH)
        .filter(|&j| {
            contents
                .versions(&batch_key(i, j))
                .iter()
                .any(|v| v.version() == i)
        })
        .count() as u64
}

fn new_store(seed: u64) -> SimStore {
    SimStore::new(Seed::new(seed), FaultConfig::none(), SimClock::new())
}

#[when("the posting-list operations of a workload run on the simulated store")]
fn run_workload(w: &mut Steps) {
    // Group 0 runs the store's own workload over posting-list keys and meta bytes; task group 2 adds the posting-list
    // operations and runs the same workload on fjall.
    w.runs = (1..=10)
        .map(|seed| StoreWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 2))
        .collect();
}

#[then("they pass the same checks as on fjall")]
fn same_checks(w: &mut Steps) {
    assert_eq!(w.runs.len(), 10);
    for run in &w.runs {
        let outcome = run.as_ref().unwrap_or_else(|v| panic!("{v}"));
        assert!(
            !outcome.contents().keys().is_empty(),
            "the workload wrote nothing"
        );
    }
}

#[when("a seed is run twice")]
fn run_twice(w: &mut Steps) {
    w.runs = (0..2)
        .map(|_| StoreWorkload::run(Seed::new(42), FaultConfig::mixed(), 400, 2))
        .collect();
}

#[then("both runs perform the same operations, inject the same faults and end in the same state")]
fn runs_equal(w: &mut Steps) {
    let first = w.runs[0].as_ref().unwrap_or_else(|v| panic!("{v}"));
    let second = w.runs[1].as_ref().unwrap_or_else(|v| panic!("{v}"));
    assert_eq!(first.trace(), second.trace());
    assert_eq!(first.contents(), second.contents());
    let faults = [
        "BatchFailed",
        "CrashedDuringBatch",
        "Crashed",
        "PowerLost",
        "Slowed",
        "ReadFailed",
    ];
    let injected: BTreeSet<&str> = faults
        .into_iter()
        .filter(|f| first.trace().iter().any(|line| line.contains(f)))
        .collect();
    assert!(injected.len() >= 3, "the run injected only {injected:?}");
}

#[when("batches are written, some synced, and a power loss is injected")]
fn power_loss(w: &mut Steps) {
    w.recovered = (0..40)
        .map(|seed| {
            let store = new_store(seed);
            let synced = 4 + seed % 5;
            for i in 1..=BATCHES {
                let mode = if i == synced {
                    PersistMode::Sync
                } else {
                    PersistMode::Buffer
                };
                store.apply(batch(i), mode).expect("no fault is configured");
            }
            store.power_loss();
            store.recover();
            (store.contents(), synced)
        })
        .collect();
}

#[then(
    "after recovery every synced batch is visible whole, the surviving batches are a prefix of batch order, no batch is visible in part, and the applied timestamp is the highest commit timestamp the store holds"
)]
fn power_loss_kept(w: &mut Steps) {
    let mut survivors = BTreeSet::new();
    for (contents, synced) in &w.recovered {
        let counts: Vec<u64> = (1..=BATCHES).map(|i| held(contents, i)).collect();
        assert!(
            counts.iter().all(|&c| c == 0 || c == KEYS_PER_BATCH),
            "a batch is visible in part: {counts:?}"
        );
        let kept = counts.iter().take_while(|&&c| c == KEYS_PER_BATCH).count() as u64;
        assert!(
            counts[kept as usize..].iter().all(|&c| c == 0),
            "not a prefix: {counts:?}"
        );
        assert!(kept >= *synced, "synced batch {synced} lost: {counts:?}");
        assert_eq!(contents.applied_ts(), kept);
        survivors.insert(kept);
    }
    assert!(
        survivors.len() > 3,
        "the fault stream kept only {survivors:?} batches"
    );
}

#[when("a batch fails during its persist, a write follows, and the store is reopened")]
fn failed_batch(w: &mut Steps) {
    w.poisoned = (0..20)
        .map(|seed| {
            let store = new_store(seed);
            store
                .apply(batch(1), PersistMode::Buffer)
                .expect("no fault is configured");
            store.inject(Fault::FailNextBatch);
            let failed = store.apply(batch(2), PersistMode::Buffer);
            assert!(
                matches!(failed, Err(StoreError(StoreErrorEnum::WriteFailed(_)))),
                "{failed:?}"
            );
            assert_eq!(
                held(&store.contents(), 2),
                0,
                "the failed batch is absent before reopening"
            );
            let refused = store.apply(batch(3), PersistMode::Buffer);
            store.recover();
            (refused, store.contents())
        })
        .collect();
}

#[then(
    "the write before reopening is refused, and after reopening the failed batch is present whole or absent whole"
)]
fn failed_batch_whole(w: &mut Steps) {
    let mut outcomes = BTreeSet::new();
    for (refused, contents) in &w.poisoned {
        assert_eq!(refused, &Err(StoreError::Poisoned()));
        let failed = held(contents, 2);
        assert!(
            failed == 0 || failed == KEYS_PER_BATCH,
            "the failed batch is visible in part"
        );
        assert_eq!(held(contents, 1), KEYS_PER_BATCH);
        assert_eq!(held(contents, 3), 0, "the refused write is absent");
        outcomes.insert(failed);
    }
    assert_eq!(outcomes.len(), 2, "both outcomes occur across the seeds");
}

#[when(
    "a drop of everything has synced its record and a power loss is injected before it deletes the record"
)]
fn interrupted_drop(w: &mut Steps) {
    let stages = [
        DropStage::RecordSynced,
        DropStage::DataRemoved,
        DropStage::EntriesSynced,
    ];
    w.recovered = stages
        .iter()
        .flat_map(|&stage| (0..5).map(move |seed| (stage, seed)))
        .map(|(stage, seed)| {
            let store = new_store(seed);
            (1..=4).for_each(|i| {
                store
                    .apply(batch(i), PersistMode::Buffer)
                    .expect("no fault is configured")
            });
            store.set_discard_ts(2).expect("no fault is configured");
            store.ban_namespace(5).expect("no fault is configured");
            store.inject(Fault::PowerLossDuringNextDrop(stage));
            let result = store.drop_all(vec![Entry::put(
                b"\x01schema".to_vec(),
                1,
                0x01,
                b"s".to_vec(),
            )]);
            assert!(result.is_err(), "the drop is interrupted at {stage:?}");
            store.recover();
            (store.contents(), 0)
        })
        .collect();
}

#[then("after recovery the store holds only the entries the drop was given")]
fn drop_finished(w: &mut Steps) {
    assert_eq!(w.recovered.len(), 15);
    for (contents, _) in &w.recovered {
        let keys: Vec<&Vec<u8>> = contents.keys().keys().collect();
        assert_eq!(keys, vec![&b"\x01schema".to_vec()]);
        assert_eq!(contents.versions(b"\x01schema")[0].value(), b"s");
        assert_eq!((contents.applied_ts(), contents.discard_ts()), (0, 0));
        assert!(contents.banned().is_empty());
    }
}

#[when("a stored posting list's bytes are corrupted and the list is read")]
fn corrupted_read(w: &mut Steps) {
    let store = new_store(1);
    let key = batch_key(1, 0);
    store
        .apply(batch(1), PersistMode::Buffer)
        .expect("no fault is configured");
    store.corrupt(&key, 1, Corruption::Truncate);
    let snapshot = store.snapshot(5).expect("no fault is configured");
    let got = snapshot.get(&key);
    let scanned = snapshot.scan(&Scan::prefix(key.clone())).collect();
    w.read = Some((key, got, scanned));
}

#[then("the read fails with an error naming the key, and no value is returned")]
fn corrupted_refused(w: &mut Steps) {
    let (key, got, scanned) = w.read.take().expect("read");
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    let error = got.expect_err("a corrupted value is refused");
    assert!(matches!(&error.0, StoreErrorEnum::Corrupted { key: k, .. } if *k == key));
    assert!(error.to_string().contains(&hex), "{error}");
    assert_eq!(scanned.len(), 1, "the scan ends at the corrupted value");
    assert!(matches!(
        &scanned[0],
        Err(StoreError(StoreErrorEnum::Corrupted { .. }))
    ));
}

#[when("a run breaks an invariant")]
fn broken_run(w: &mut Steps) {
    // A workload that writes a key below its own deletion.
    let mut simulation = Simulation::new(Seed::new(99), FaultConfig::none());
    let mut writes = 0u64;
    w.violation = Some(simulation.run(10, 1, |ctx, event| {
        if let Event::Step(_) = event {
            writes += 1;
            let mut batch = WriteBatch::new();
            batch.push(match writes {
                1 => Entry::deletion(b"\x00key".to_vec(), 10),
                2 => Entry::put(b"\x00key".to_vec(), 5, 0x08, b"v".to_vec()),
                n => Entry::put(b"\x00other".to_vec(), 10 + n, 0x04, b"v".to_vec()),
            });
            let _ = ctx.store().apply(batch, PersistMode::Buffer);
        }
        Ok(())
    }));
}

#[then("the simulator stops and reports the invariant, the step and the seed")]
fn reported(w: &mut Steps) {
    let violation = w
        .violation
        .take()
        .expect("ran")
        .expect_err("the run breaks an invariant");
    assert_eq!(violation.invariant(), Invariant::NoWriteBelowDeletion);
    assert_eq!((violation.seed(), violation.step()), (Seed::new(99), 1));
    let text = violation.to_string();
    assert!(
        text.starts_with("seed 99 step 1 broke the invariant that no write lands below a deletion"),
        "{text}"
    );
    assert!(text.ends_with("replay it with seed 99"), "{text}");
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
