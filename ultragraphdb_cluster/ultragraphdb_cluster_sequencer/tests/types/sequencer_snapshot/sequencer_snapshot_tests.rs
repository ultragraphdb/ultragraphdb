/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_cluster_sequencer::{Close, SequencedTransaction, SequencerSnapshot};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::Position;

/// The close of `epoch` at `time_us`.
fn close(epoch: u64, time_us: u64) -> TestResult<Close> {
    Ok(Close::new(
        Epoch::new(epoch)?,
        time_us,
        Vec::new(),
        Vec::new(),
    )?)
}

/// A transaction placed in `epoch`.
fn transaction(epoch: u64) -> TestResult<SequencedTransaction> {
    Ok(SequencedTransaction::new(
        Epoch::new(epoch)?,
        Epoch::new(epoch)?,
        vec![PartitionId::new(1)?],
        vec![1],
    )?)
}

/// The refusal of a snapshot of these parts.
fn refused(
    position: u64,
    closes: Vec<Close>,
    transactions: Vec<(Position, SequencedTransaction)>,
) -> TestResult<ClusterErrorEnum> {
    Ok(
        SequencerSnapshot::new(Position::new(position), closes, transactions)
            .err()
            .context("a refusal")?
            .0,
    )
}

#[test]
fn test_a_snapshot_holds_its_parts() -> TestResult {
    let closes = vec![close(3, 10)?, close(4, 10)?];
    let transactions = vec![
        (Position::new(2), transaction(4)?),
        (Position::new(5), transaction(5)?),
    ];
    let snapshot = SequencerSnapshot::new(Position::new(5), closes.clone(), transactions.clone())?;
    assert_eq!(snapshot.position(), Position::new(5));
    assert_eq!(snapshot.closes(), closes.as_slice());
    assert_eq!(snapshot.transactions(), transactions.as_slice());
    let empty = SequencerSnapshot::new(Position::ZERO, Vec::new(), Vec::new())?;
    assert!(empty.closes().is_empty() && empty.transactions().is_empty());
    Ok(())
}

#[test]
fn test_parts_out_of_order_are_refused() -> TestResult {
    assert_eq!(
        refused(5, vec![close(3, 10)?, close(5, 10)?], Vec::new())?,
        ClusterError::malformed("a close one after the one before it", 1).0
    );
    assert_eq!(
        refused(5, vec![close(3, 10)?, close(4, 9)?], Vec::new())?,
        ClusterError::malformed("a close time at or above the one before it", 1).0
    );
    let t = transaction(4)?;
    assert_eq!(
        refused(
            5,
            Vec::new(),
            vec![(Position::new(3), t.clone()), (Position::new(3), t.clone())]
        )?,
        ClusterError::malformed("a transaction after the one before it", 1).0
    );
    assert_eq!(
        refused(5, Vec::new(), vec![(Position::new(6), t.clone())])?,
        ClusterError::malformed("a transaction at or before the snapshot's position", 0).0
    );
    assert_eq!(
        refused(5, Vec::new(), vec![(Position::ZERO, t)])?,
        ClusterError::malformed("a transaction after the one before it", 0).0
    );
    Ok(())
}
