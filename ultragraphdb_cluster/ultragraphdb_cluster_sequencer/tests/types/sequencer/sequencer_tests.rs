/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_sequencer::ClockReading;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_error_log::{LogError, LogErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::OrderedLog;
use ultragraphdb_transactions_log::FaultMode;

use crate::utils::harness::{at, block_on, config, fresh, pid, recover};

#[test]
fn test_a_fresh_sequencer_has_closed_no_epoch() -> TestResult {
    let (_, sequencer) = fresh(config(2, 10)?)?;
    assert_eq!(sequencer.last_closed().get(), 0);
    assert_eq!(sequencer.config().epoch_ms_max(), 10);
    Ok(())
}

#[test]
fn test_the_first_close_is_due_epoch_ms_max_after_the_start() -> TestResult {
    let storage = ultragraphdb_transactions_log::SimStorage::new(3);
    // Started at monotonic 7 ms, wall time unrelated.
    let sequencer = recover(&storage, config(2, 10)?, ClockReading::new(7_000, 99))?;
    assert_eq!(sequencer.deadline(), 17_000);
    Ok(())
}

#[test]
fn test_a_close_counts_only_once_synced() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    sequencer.subscribe(pid(1)?, ultragraphdb_core_ids::Epoch::new(0)?)?;
    let closed = sequencer.tick(at(10), &[], 0, &[])?;
    assert_eq!(closed.map(|e| e.get()), Some(1));
    assert_eq!(sequencer.last_closed().get(), 0);
    assert_eq!(sequencer.take_delivery(pid(1)?)?, None);
    let durable = block_on(sequencer.sync())??;
    assert_eq!(durable.get(), 1);
    assert_eq!(sequencer.last_closed().get(), 1);
    let delivered = sequencer
        .take_delivery(pid(1)?)?
        .context("the close of 1")?;
    assert_eq!(delivered.epoch().get(), 1);
    Ok(())
}

#[test]
fn test_a_sync_with_nothing_appended_reports_the_durable_position() -> TestResult {
    let (_, mut sequencer) = fresh(config(2, 10)?)?;
    assert_eq!(block_on(sequencer.sync())??.get(), 0);
    let _closed = sequencer.tick(at(10), &[], 0, &[])?;
    assert_eq!(block_on(sequencer.sync())??.get(), 1);
    assert_eq!(block_on(sequencer.sync())??.get(), 1);
    Ok(())
}

#[test]
fn test_a_failed_sync_is_the_logs_error_and_nothing_is_delivered() -> TestResult {
    let (storage, mut sequencer) = fresh(config(2, 10)?)?;
    sequencer.subscribe(pid(1)?, ultragraphdb_core_ids::Epoch::new(0)?)?;
    let _closed = sequencer.tick(at(10), &[], 0, &[])?;
    // Every storage operation from here on fails before it takes effect.
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    let refusal = block_on(sequencer.sync())?.err().context("a refusal")?;
    // The sync's first storage operation, creating the log's first segment, is the one failed.
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::Log(LogError(LogErrorEnum::Io {
            operation: "create",
            kind: std::io::ErrorKind::Other,
            message: "injected fault before the operation".to_string(),
        }))
    );
    assert_eq!(sequencer.last_closed().get(), 0);
    assert_eq!(sequencer.take_delivery(pid(1)?)?, None);
    // The log is poisoned: the next close is refused by it.
    let refused = sequencer
        .tick(at(20), &[], 0, &[])
        .err()
        .context("a refusal")?;
    assert!(
        matches!(refused.0, ClusterErrorEnum::Log(ref e) if e.0 == LogErrorEnum::Poisoned),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn test_the_log_is_the_sequencers_own() -> TestResult {
    // Every close is one round of the log, its version the epoch.
    let (storage, mut sequencer) = fresh(config(1, 1)?)?;
    for ms in 1..=3 {
        let _closed = sequencer.tick(at(ms), &[], 0, &[])?;
    }
    let _durable = block_on(sequencer.sync())??;
    let mut log = crate::utils::harness::open(&storage)?;
    let rounds = block_on(log.read(ultragraphdb_traits_log::Position::new(1), 1 << 20))??;
    let versions: Vec<u64> = rounds.iter().map(|r| r.first_version()).collect();
    assert_eq!(versions, vec![1, 2, 3]);
    assert!(rounds.iter().all(|r| r.transactions() == 0));
    Ok(())
}
