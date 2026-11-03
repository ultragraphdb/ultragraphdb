/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Recovery in the simulator: the sequencer's workload of closes and multi-partition transactions over the
//! write-ahead log on its simulated storage, killed after every append, before and after its sync, and with every
//! storage operation of the run failed in each crash mode. After each kill the recovered state must be a prefix of
//! what was appended that holds everything acknowledged, and the sequencer must go on closing from it.

use ultragraphdb_cluster_sequencer::{
    CatalogVersion, ClockReading, Close, SequencedTransaction, SequencerConfig, SequencerRecord,
};
use ultragraphdb_core_ids::{Epoch, Namespace};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{FaultMode, SimStorage};

use crate::utils::harness::{
    SequencerOnLog, block_on, config, crash_and_recover, drain, epoch, fresh, pid,
};

/// The workload's steps: three closes, then a transaction, four times over.
const STEPS: usize = 16;

/// What the workload appended, in log order, and how many of those records were acknowledged durable.
#[derive(Debug, Default)]
struct Model {
    appended: Vec<SequencerRecord>,
    acknowledged: usize,
}

fn settings() -> TestResult<SequencerConfig> {
    config(1, 2)
}

/// Step `step` of the workload: a close at the deadline, announcing a version on every fifth step, or a transaction.
fn step(
    sequencer: &mut SequencerOnLog,
    step: usize,
    model: &mut Model,
) -> TestResult<Result<(), ClusterError>> {
    let index = u64::try_from(step)?;
    if step % 4 == 3 {
        let partitions = [pid(1 + u32::try_from(step % 3)?)?];
        let reply = sequencer.last_closed();
        let (placed, _) = match sequencer.submit(reply, &partitions, &[u8::try_from(step)?]) {
            Ok(placed) => placed,
            Err(refusal) => return Ok(Err(refusal)),
        };
        let record = SequencedTransaction::new(
            placed,
            reply,
            partitions.to_vec(),
            vec![u8::try_from(step)?],
        )?;
        model.appended.push(SequencerRecord::Transaction(record));
        return Ok(Ok(()));
    }
    let catalog: Vec<_> = step
        .is_multiple_of(5)
        .then(|| CatalogVersion::new(Namespace::new(1), index))
        .into_iter()
        .collect();
    let now = sequencer.deadline();
    let closed = match sequencer.tick(
        ClockReading::new(now, now),
        &catalog,
        0,
        &[u128::from(index)],
    ) {
        Ok(closed) => closed.context("a close at its deadline")?,
        Err(refusal) => return Ok(Err(refusal)),
    };
    model.appended.push(SequencerRecord::Close(Close::new(
        closed,
        now,
        catalog,
        vec![u128::from(index)],
    )?));
    Ok(Ok(()))
}

/// Runs the workload until `appends` records are appended, syncing after each, the last only when `sync_last`; stops
/// at the first refusal, which a fault causes.
fn run(
    sequencer: &mut SequencerOnLog,
    model: &mut Model,
    appends: usize,
    sync_last: bool,
) -> TestResult {
    for index in 0..STEPS {
        if step(sequencer, index, model)?.is_err() {
            return Ok(());
        }
        if model.appended.len() == appends && !sync_last {
            return Ok(());
        }
        match block_on(sequencer.sync())? {
            Ok(durable) => model.acknowledged = usize::try_from(durable.get())?,
            Err(_) => return Ok(()),
        }
        if model.appended.len() == appends {
            return Ok(());
        }
    }
    Ok(())
}

/// The records the recovered sequencer holds, closes and transactions apart.
fn recovered(
    sequencer: &mut SequencerOnLog,
) -> TestResult<(Vec<Close>, Vec<SequencedTransaction>)> {
    sequencer.subscribe(pid(9)?, epoch(0)?)?;
    let closes = drain(sequencer, pid(9)?)?;
    let transactions = sequencer
        .transactions_from(epoch(0)?)
        .into_iter()
        .cloned()
        .collect();
    Ok((closes, transactions))
}

/// Checks that the recovered records are the appended records' first `p`, for some `p` from the acknowledged count on,
/// and that the recovered sequencer closes the next epoch.
fn check(storage: &SimStorage, model: &Model, case: &str) -> TestResult {
    let mut sequencer = crash_and_recover(
        storage,
        settings()?,
        ClockReading::new(1_000_000, 1_000_000),
    )?;
    let (closes, transactions) = recovered(&mut sequencer)?;
    let matches = (model.acknowledged..=model.appended.len()).any(|p| {
        let prefix = model.appended.get(..p).unwrap_or_default();
        let want_closes: Vec<&Close> = prefix
            .iter()
            .filter_map(|r| match r {
                SequencerRecord::Close(c) => Some(c),
                SequencerRecord::Transaction(_) => None,
            })
            .collect();
        let want_transactions: Vec<&SequencedTransaction> = prefix
            .iter()
            .filter_map(|r| match r {
                SequencerRecord::Transaction(t) => Some(t),
                SequencerRecord::Close(_) => None,
            })
            .collect();
        closes.iter().collect::<Vec<_>>() == want_closes
            && transactions.iter().collect::<Vec<_>>() == want_transactions
    });
    if !matches {
        return Err(TestError::new(format!(
            "{case}: recovered {} closes and {} transactions, appended {:?}, acknowledged {}",
            closes.len(),
            transactions.len(),
            model.appended,
            model.acknowledged
        )));
    }
    let last = sequencer.last_closed().get();
    assert_eq!(last, closes.last().map_or(0, |c| c.epoch().get()), "{case}");
    let now = sequencer.deadline();
    let next = sequencer.tick(ClockReading::new(now, now), &[], 0, &[])?;
    assert_eq!(next.map(Epoch::get), Some(last + 1), "{case}");
    let _durable = block_on(sequencer.sync())??;
    Ok(())
}

#[test]
fn test_a_kill_after_every_append_before_its_sync_loses_only_it() -> TestResult {
    for appends in 1..=STEPS {
        let (storage, mut sequencer) = fresh(settings()?)?;
        let mut model = Model::default();
        run(&mut sequencer, &mut model, appends, false)?;
        assert_eq!(model.appended.len(), appends);
        assert_eq!(model.acknowledged, appends - 1);
        check(
            &storage,
            &model,
            &format!("killed before the sync of append {appends}"),
        )?;
    }
    Ok(())
}

#[test]
fn test_a_kill_after_every_synced_append_keeps_it() -> TestResult {
    for appends in 1..=STEPS {
        let (storage, mut sequencer) = fresh(settings()?)?;
        let mut model = Model::default();
        run(&mut sequencer, &mut model, appends, true)?;
        assert_eq!(model.acknowledged, appends);
        check(
            &storage,
            &model,
            &format!("killed after the sync of append {appends}"),
        )?;
    }
    Ok(())
}

#[test]
fn test_a_fault_at_every_storage_operation_then_a_kill_recovers_a_prefix() -> TestResult {
    // The trace of a clean run bounds the operations a fault can hit.
    let (storage, mut sequencer) = fresh(settings()?)?;
    let start = storage.trace().len();
    run(&mut sequencer, &mut Model::default(), STEPS, true)?;
    let end = storage.trace().len();
    assert!(end > start + STEPS, "{start}..{end}");
    let modes = [
        FaultMode::Before,
        FaultMode::After,
        FaultMode::TornWrite,
        FaultMode::ShortTornWrite,
    ];
    for operation in start..end {
        for mode in modes {
            let (faulted_storage, mut faulted) = fresh(settings()?)?;
            faulted_storage.fail_at(operation, mode);
            let mut model = Model::default();
            run(&mut faulted, &mut model, STEPS, true)?;
            check(
                &faulted_storage,
                &model,
                &format!("{mode:?} at operation {operation}"),
            )?;
        }
    }
    Ok(())
}
