/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Kills a process writing into a fjall store during a batch, and reopens the store: a batch is present whole or
//! absent whole, and the batches present are a prefix of batch order with the applied timestamp of the last. The
//! drop, temporary-keyspace and rebuild-record kills are targets of their own, so Bazel runs them in parallel.

use std::time::Duration;

use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::{
    DROP_STAGES, KEYS_PER_BATCH, RECORD_STAGES, TEMPORARY_STAGES, acknowledged_rounds,
    batch_counts, fresh_dir, kill_after, kill_at, open, replay_round,
};
use ultragraphdb_traits_store::Store;

#[test]
fn test_a_kill_during_a_batch_leaves_it_whole_or_absent() -> TestResult {
    let mut in_flight = 0;
    for (round, delay) in [0u64, 1, 2, 3, 5, 8].into_iter().enumerate() {
        let dir = fresh_dir(&format!("batch-{round}"));
        let lines = kill_after(&dir, "batches", "batch 3", Duration::from_millis(delay))?;
        let started = lines.iter().filter(|l| l.starts_with("batch ")).count() as u64;
        let applied = lines.iter().filter(|l| l.starts_with("applied ")).count() as u64;
        if started > applied {
            in_flight += 1;
        }
        let store = open(&dir)?;
        let counts = batch_counts(&store, started + 1)?;
        assert!(
            counts.iter().all(|&c| c == 0 || c == KEYS_PER_BATCH),
            "round {round}: a batch is visible in part: {counts:?}"
        );
        let present = counts.iter().take_while(|&&c| c == KEYS_PER_BATCH).count() as u64;
        assert!(
            counts[present as usize..].iter().all(|&c| c == 0),
            "round {round}: not a prefix: {counts:?}"
        );
        assert!(
            present >= applied,
            "round {round}: an applied batch was lost: {counts:?}, {applied} applied"
        );
        assert_eq!(
            store.applied_ts(),
            present,
            "round {round}: the applied timestamp names the last batch held"
        );
        drop(store);
        // A directory left behind is litter in the temporary directory, not a failure of the test.
        drop(std::fs::remove_dir_all(&dir));
    }
    println!("{in_flight} of 6 kills landed while a batch was written");
    Ok(())
}

/// In log mode the write-ahead log is the only synced stream: a kill before the store's journal is synced leaves the
/// store a prefix of its batches, each whole, never past the rounds the log acknowledged, with the applied position of
/// the last; replaying the log above it restores every acknowledged round.
#[test]
fn test_a_kill_in_log_mode_loses_no_acknowledged_round() -> TestResult {
    let mut replayed = 0;
    for (round, delay) in [0u64, 1, 3, 8].into_iter().enumerate() {
        let dir = fresh_dir(&format!("log-{round}"));
        let lines = kill_after(&dir, "log", "acked 3", Duration::from_millis(delay))?;
        let announced = lines.iter().filter(|l| l.starts_with("acked ")).count() as u64;
        let acknowledged = acknowledged_rounds(&dir)?;
        assert!(
            acknowledged >= announced,
            "round {round}: the log lost an acknowledged round"
        );
        let store = FjallStore::open(&dir, Durability::Log).context("the store reopens")?;
        let counts = batch_counts(&store, acknowledged + 1)?;
        assert!(
            counts.iter().all(|&c| c == 0 || c == KEYS_PER_BATCH),
            "round {round}: a batch is visible in part: {counts:?}"
        );
        let present = counts.iter().take_while(|&&c| c == KEYS_PER_BATCH).count() as u64;
        assert!(
            counts.iter().skip(present as usize).all(|&c| c == 0),
            "round {round}: not a prefix: {counts:?}"
        );
        assert!(
            present <= acknowledged,
            "round {round}: the store is ahead of the log"
        );
        assert_eq!(
            store.applied_ts(),
            present,
            "round {round}: the applied position names the last batch"
        );
        for lost in present + 1..=acknowledged {
            replay_round(&store, lost)?;
            replayed += 1;
        }
        let restored = batch_counts(&store, acknowledged + 1)?;
        let held = restored
            .iter()
            .take_while(|&&c| c == KEYS_PER_BATCH)
            .count() as u64;
        assert_eq!(held, acknowledged, "round {round}: {restored:?}");
        assert_eq!(store.applied_ts(), acknowledged, "round {round}");
        drop(store);
        // A directory left behind is litter in the temporary directory, not a failure of the test.
        drop(std::fs::remove_dir_all(&dir));
        drop(std::fs::remove_file(dir.with_extension("rounds")));
    }
    println!("{replayed} acknowledged rounds replayed from the log over 4 kills");
    Ok(())
}

/// The writer announces every stage of its drop, temporary and record modes, in order, when it runs to the last.
#[test]
fn test_the_writer_announces_each_stage_it_reaches() -> TestResult {
    for (mode, stages) in [
        ("drop", DROP_STAGES.as_slice()),
        ("temporary", TEMPORARY_STAGES.as_slice()),
        ("record", RECORD_STAGES.as_slice()),
    ] {
        let dir = fresh_dir(&format!("stages-{mode}"));
        let last = stages.last().context("a mode has stages")?;
        let announced: Vec<String> = kill_at(&dir, mode, last)?
            .into_iter()
            .filter(|l| l.starts_with("stage "))
            .collect();
        let expected: Vec<String> = stages.iter().map(|s| format!("stage {s}")).collect();
        assert_eq!(announced, expected, "{mode}");
        // A directory left behind is litter in the temporary directory, not a failure of the test.
        drop(std::fs::remove_dir_all(&dir));
    }
    Ok(())
}
