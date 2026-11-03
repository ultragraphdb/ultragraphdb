/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The sequencer's log snapshotted and trimmed (task 6.4): the snapshot holds the durable closes retained and the
//! transactions held, the log is trimmed through it, and recovery replays only the rounds after it.

use std::num::NonZeroU32;

use ultragraphdb_cluster_sequencer::{ClockReading, Sequencer, SequencerSnapshot};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_error_log::LogError;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::SimStorage;

use crate::utils::harness::{
    at, block_on, close_through, config, crash_and_recover, drain, epoch, fresh, open, pid,
};

/// Recovers from the log of `storage` and `snapshot`, started at `now`.
fn recover_from(
    storage: &SimStorage,
    snapshot: Option<SequencerSnapshot>,
) -> TestResult<Result<Sequencer<crate::utils::harness::Log>, ClusterError>> {
    block_on(Sequencer::recover_from(
        open(storage)?,
        config(1, 2)?,
        at(1_000),
        snapshot,
    ))
}

#[test]
fn test_a_snapshot_holds_the_durable_closes_and_transactions() -> TestResult {
    let (_storage, mut sequencer) = fresh(config(1, 2)?)?;
    let _ms = close_through(&mut sequencer, 3)?;
    let (_, placed) = sequencer.submit(epoch(3)?, &[pid(1)?], b"t")?;
    let _durable = block_on(sequencer.sync())??;
    // A close appended and not yet durable is not in the snapshot.
    let now = sequencer.deadline();
    let _closed = sequencer.tick(ClockReading::new(now, now), &[], 0, &[])?;
    let snapshot = sequencer.snapshot();
    assert_eq!(snapshot.position().get(), 4);
    let epochs: Vec<u64> = snapshot.closes().iter().map(|c| c.epoch().get()).collect();
    assert_eq!(epochs, vec![1, 2, 3]);
    let transactions: Vec<(u64, u64)> = snapshot
        .transactions()
        .iter()
        .map(|(p, t)| (p.get(), t.epoch().get()))
        .collect();
    assert_eq!(transactions, vec![(placed.get(), 4)]);
    Ok(())
}

#[test]
fn test_a_snapshot_holds_only_the_closes_retained() -> TestResult {
    let config = config(1, 2)?.with_closes_retained(NonZeroU32::MIN.saturating_add(1));
    let (_storage, mut sequencer) = fresh(config)?;
    let _ms = close_through(&mut sequencer, 5)?;
    let epochs: Vec<u64> = sequencer
        .snapshot()
        .closes()
        .iter()
        .map(|c| c.epoch().get())
        .collect();
    assert_eq!(epochs, vec![4, 5]);
    Ok(())
}

#[test]
fn test_recovery_from_a_snapshot_replays_the_rounds_after_it_and_equals_a_full_replay() -> TestResult
{
    let (storage, mut sequencer) = fresh(config(1, 2)?)?;
    let _ms = close_through(&mut sequencer, 3)?;
    let _t = sequencer.submit(epoch(3)?, &[pid(1)?, pid(2)?], b"a")?;
    let _durable = block_on(sequencer.sync())??;
    let snapshot = sequencer.snapshot();
    let mut stored = Vec::new();
    snapshot.encode(&mut stored);
    block_on(sequencer.trim(&snapshot))??;
    let _ms_2 = close_through(&mut sequencer, 6)?;
    let _t_2 = sequencer.submit(epoch(6)?, &[pid(3)?], b"b")?;
    let _durable_2 = block_on(sequencer.sync())??;
    storage.power_loss();
    let mut recovered = recover_from(&storage, Some(SequencerSnapshot::decode(&stored)?))??;
    assert_eq!(recovered.last_closed().get(), 6);
    let bodies: Vec<Vec<u8>> = recovered
        .transactions_from(epoch(0)?)
        .iter()
        .map(|t| t.body().to_vec())
        .collect();
    assert_eq!(bodies, vec![b"a".to_vec(), b"b".to_vec()]);
    recovered.subscribe(pid(1)?, epoch(0)?)?;
    let epochs: Vec<u64> = drain(&mut recovered, pid(1)?)?
        .iter()
        .map(|c| c.epoch().get())
        .collect();
    assert_eq!(epochs, (1..=6).collect::<Vec<u64>>());
    // It goes on closing from 7.
    let now = recovered.deadline();
    assert_eq!(
        recovered
            .tick(ClockReading::new(now, now), &[], 0, &[])?
            .map(|e| e.get()),
        Some(7)
    );
    Ok(())
}

#[test]
fn test_a_trimmed_log_needs_a_snapshot_at_or_past_its_checkpoint() -> TestResult {
    let (storage, mut sequencer) = fresh(config(1, 2)?)?;
    let _ms = close_through(&mut sequencer, 2)?;
    let old = sequencer.snapshot();
    let _ms_2 = close_through(&mut sequencer, 4)?;
    let snapshot = sequencer.snapshot();
    block_on(sequencer.trim(&snapshot))??;
    // A second trim at the same snapshot changes nothing.
    block_on(sequencer.trim(&snapshot))??;
    drop(sequencer);
    let refused = |offered| -> TestResult<ClusterErrorEnum> {
        Ok(recover_from(&storage, offered)?
            .err()
            .context("a refusal")?
            .0)
    };
    assert_eq!(refused(None)?, ClusterError::snapshot_behind(0, 4).0);
    assert_eq!(refused(Some(old))?, ClusterError::snapshot_behind(2, 4).0);
    // The plain recovery is a recovery without a snapshot.
    let plain = crash_and_recover(&storage, config(1, 2)?, at(1_000))
        .err()
        .context("a refusal")?;
    assert!(
        plain.reason().contains("checkpoint 4"),
        "{}",
        plain.reason()
    );
    let recovered = recover_from(&storage, Some(snapshot))??;
    assert_eq!(recovered.last_closed().get(), 4);
    Ok(())
}

#[test]
fn test_a_snapshot_past_the_committed_position_is_refused() -> TestResult {
    let storage = SimStorage::new(2);
    let snapshot = SequencerSnapshot::new(Position::new(3), Vec::new(), Vec::new())?;
    let refusal = recover_from(&storage, Some(snapshot))?
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterError::replay_diverged(3, "a snapshot at or below the log's committed position").0
    );
    Ok(())
}

#[test]
fn test_a_trim_past_the_committed_position_is_the_logs_refusal() -> TestResult {
    let (_storage, mut sequencer) = fresh(config(1, 2)?)?;
    let _ms = close_through(&mut sequencer, 1)?;
    let ahead = SequencerSnapshot::new(Position::new(9), Vec::new(), Vec::new())?;
    let refusal = block_on(sequencer.trim(&ahead))?
        .err()
        .context("a refusal")?;
    // The log's committed position is 1, the close of epoch 1.
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::Log(LogError::position_mismatch(1, 9))
    );
    Ok(())
}
