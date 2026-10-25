/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{FaultConfig, Seed, StoreWorkload};

#[test]
fn test_the_workload_has_an_apply_loop_and_its_readers() {
    assert_eq!(StoreWorkload::new(3).actors(), 4);
    assert_eq!(StoreWorkload::new(0).actors(), 1);
}

#[test]
fn test_a_workload_without_readers_only_writes() {
    let outcome = StoreWorkload::run(Seed::new(5), FaultConfig::none(), 200, 0).unwrap();
    let steps: Vec<&String> = outcome
        .trace()
        .iter()
        .filter(|l| l.starts_with("step "))
        .collect();
    assert_eq!(steps.len(), 200);
    assert!(
        steps.iter().all(|l| l.ends_with(" picks 0")),
        "the apply loop is the only actor: {steps:?}"
    );
    for read in ["SnapshotOpened", "Read", "ReadMany", "Scanned"] {
        assert!(
            !outcome.trace().iter().any(|l| l.starts_with(read)),
            "{read} without a reader"
        );
    }
    assert!(outcome.trace().iter().any(|l| l.starts_with("Applied")));
}

/// Whether the trace line `line` is a record of the variant `kind`, and not of another whose name starts with it.
fn is_record(line: &str, kind: &str) -> bool {
    line == kind || line.starts_with(&format!("{kind} {{"))
}

#[test]
fn test_a_run_writes_reads_and_replays() {
    let first = StoreWorkload::run(Seed::new(11), FaultConfig::mixed(), 250, 2).unwrap();
    let again = StoreWorkload::run(Seed::new(11), FaultConfig::mixed(), 250, 2).unwrap();
    assert_eq!(first, again);
    let other = StoreWorkload::run(Seed::new(12), FaultConfig::mixed(), 250, 2).unwrap();
    assert_ne!(first.trace(), other.trace());
    let kinds = [
        "Applied",
        "Scanned",
        "Read",
        "ReadMany",
        "Dropped",
        "Compacted",
        "Swept",
        "Banned",
        "Refused",
        "RebuildRecorded",
        "TemporaryCreated",
        "TemporaryApplied",
        "TemporarySnapshotOpened",
        "TemporaryDropped",
    ];
    let all: Vec<String> = (0..6)
        .flat_map(|s| {
            StoreWorkload::run(Seed::new(s), FaultConfig::none(), 300, 2)
                .unwrap()
                .trace()
                .to_vec()
        })
        .collect();
    for kind in kinds {
        assert!(
            all.iter().any(|l| is_record(l, kind)),
            "no {kind} in six runs"
        );
    }
}

#[test]
fn test_mixed_faults_stop_rebuilds_and_fail_temporary_writes() {
    let all: Vec<String> = (0..24)
        .flat_map(|s| {
            StoreWorkload::run(Seed::new(s), FaultConfig::mixed(), 300, 2)
                .unwrap()
                .trace()
                .to_vec()
        })
        .collect();
    for kind in ["RebuildInterrupted", "TemporaryFailed"] {
        assert!(
            all.iter().any(|l| is_record(l, kind)),
            "no {kind} in 24 runs"
        );
    }
}
