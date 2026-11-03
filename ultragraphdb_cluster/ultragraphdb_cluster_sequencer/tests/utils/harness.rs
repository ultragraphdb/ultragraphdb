/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The sequencer over the write-ahead log on its simulated storage: opening, crashing and reopening it, a clock in
//! milliseconds, and driving closes. Shared by the unit tests and, by path, the BDD target.

use std::num::NonZeroU32;

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_cluster_sequencer::{
    ClockReading, Close, Sequencer, SequencerConfig, SequencerRecord,
};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_log::{OrderedLog, RoundRef};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};

/// The sequencer's log directory in the simulated storage, under its root as the frontend's `/data/seq` is under
/// `/data`.
pub const DIR: &str = "/seq";

/// The log every test runs the sequencer over.
pub type Log = Wal<SimStorage>;

/// The sequencer over [`Log`].
pub type SequencerOnLog = Sequencer<Log>;

/// A small log: segments of 64 blocks, 256 KiB, at most 64 of them.
pub fn wal_config() -> WalConfig {
    WalConfig::new(1, false)
        .with_segment_blocks(NonZeroU32::MIN.saturating_add(63))
        .with_capacity(NonZeroU32::MIN.saturating_add(63))
}

/// Runs a future of the simulated storage, which completes when first polled, by polling it with a waker that does
/// nothing; a future still pending after a bounded number of polls fails the test.
pub fn block_on<F: Future>(future: F) -> TestResult<F::Output> {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    for _ in 0..1_000 {
        if let std::task::Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return Ok(output);
        }
    }
    Err(TestError::new(
        "a simulated future still pending after 1,000 polls",
    ))
}

/// The clock at `ms` milliseconds, monotonic and wall alike.
pub fn at(ms: u64) -> ClockReading {
    ClockReading::new(ms * 1_000, ms * 1_000)
}

/// The settings with `min` and `max` milliseconds between closes.
pub fn config(min: u64, max: u64) -> TestResult<SequencerConfig> {
    Ok(SequencerConfig::new(min, max)?)
}

/// The partition `partition`.
pub fn pid(partition: u32) -> TestResult<PartitionId> {
    Ok(PartitionId::new(partition)?)
}

/// The epoch `epoch`.
pub fn epoch(epoch: u64) -> TestResult<Epoch> {
    Ok(Epoch::new(epoch)?)
}

/// The log in [`DIR`] of `storage`, opened and recovered.
pub fn open(storage: &SimStorage) -> TestResult<Log> {
    let (log, _) = block_on(Wal::open(storage.clone(), DIR, wal_config()))??;
    Ok(log)
}

/// The sequencer recovered from the log of `storage`, started at `now`.
pub fn recover(
    storage: &SimStorage,
    config: SequencerConfig,
    now: ClockReading,
) -> TestResult<Sequencer<Log>> {
    Ok(block_on(Sequencer::recover(open(storage)?, config, now))??)
}

/// A fresh sequencer over an empty log, started at time 0.
pub fn fresh(config: SequencerConfig) -> TestResult<(SimStorage, Sequencer<Log>)> {
    let storage = SimStorage::new(7);
    let sequencer = recover(&storage, config, at(0))?;
    Ok((storage, sequencer))
}

/// Kills the process: the power is cut, so every write not synced is lost, and the sequencer is recovered from what
/// the log holds, started at `now`.
pub fn crash_and_recover(
    storage: &SimStorage,
    config: SequencerConfig,
    now: ClockReading,
) -> TestResult<Sequencer<Log>> {
    storage.power_loss();
    recover(storage, config, now)
}

/// Ticks `sequencer` at each of its deadlines, syncing after each close, until epoch `through` is durable; returns the
/// monotonic time in milliseconds of the last close.
pub fn close_through(sequencer: &mut Sequencer<Log>, through: u64) -> TestResult<u64> {
    let mut now_ms = 0;
    for _ in 0..through.saturating_mul(2).saturating_add(2) {
        if sequencer.last_closed().get() >= through {
            return Ok(now_ms);
        }
        now_ms = sequencer.deadline() / 1_000;
        let _closed: Option<Epoch> = sequencer.tick(at(now_ms), &[], 0, &[])?;
        let _durable = block_on(sequencer.sync())??;
    }
    Err(TestError::new(format!("epoch {through} did not close")))
}

/// Every durable close not yet delivered to `partition`, in order.
pub fn drain(sequencer: &mut Sequencer<Log>, partition: PartitionId) -> TestResult<Vec<Close>> {
    let mut closes = Vec::new();
    for _ in 0..100_000 {
        match sequencer.take_delivery(partition)? {
            Some(close) => closes.push(close),
            None => return Ok(closes),
        }
    }
    Err(TestError::new("more than 100,000 closes delivered"))
}

/// Writes `records` as the rounds of a fresh log in `storage`, synced, as a log the sequencer wrote would hold them.
pub fn write_log(storage: &SimStorage, records: &[SequencerRecord]) -> TestResult {
    let mut log = open(storage)?;
    for record in records {
        let mut body = Vec::new();
        record.encode(&mut body);
        let (tail, parent) = log.tail();
        let position = tail.next().context("a next position")?;
        let transactions = match record {
            SequencerRecord::Close(_) => 0,
            SequencerRecord::Transaction(_) => 1,
        };
        let round = RoundRef::new(
            position,
            parent,
            record.epoch().get(),
            transactions,
            NonZeroU32::MIN,
            &body,
        );
        let _appended = log.append(round)?;
    }
    let _durable = block_on(log.sync())??;
    Ok(())
}
