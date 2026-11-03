/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The metadata group over the write-ahead log on its simulated storage: opening, crashing and reopening it with the
//! snapshot the host stored, and the values the tests name. Shared by the unit tests and, by path, the BDD target.

use std::num::NonZeroU32;

use ultragraphdb_cluster_metadata::{MetadataConfig, MetadataGroup, MetadataSnapshot};
use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId};
use ultragraphdb_core_ids::{Epoch, Namespace};
use ultragraphdb_protocol_scram::{MIN_ITERATIONS, ScramVerifier};
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};

/// The group's log directory in the simulated storage, under its root as the frontend's `/data/meta` is under `/data`.
pub const DIR: &str = "/meta";

/// The log every test runs the group over.
pub type Log = Wal<SimStorage>;

/// The group over [`Log`].
pub type Group = MetadataGroup<Log>;

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

/// The log in [`DIR`] of `storage`, opened and recovered.
pub fn open(storage: &SimStorage) -> TestResult<Log> {
    let (log, _) = block_on(Wal::open(storage.clone(), DIR, wal_config()))??;
    Ok(log)
}

/// The group recovered from the log of `storage` and `snapshot`.
pub fn recover(
    storage: &SimStorage,
    config: MetadataConfig,
    snapshot: Option<MetadataSnapshot>,
) -> TestResult<Group> {
    Ok(block_on(Group::recover(open(storage)?, config, snapshot))??)
}

/// A fresh group over an empty log, with the default settings.
pub fn fresh() -> TestResult<(SimStorage, Group)> {
    fresh_with(MetadataConfig::new())
}

/// A fresh group over an empty log, with `config`.
pub fn fresh_with(config: MetadataConfig) -> TestResult<(SimStorage, Group)> {
    let storage = SimStorage::new(11);
    let group = recover(&storage, config, None)?;
    Ok((storage, group))
}

/// Kills the process: the power is cut, so every write not synced is lost, and the group is recovered from what the
/// log holds and `snapshot`, the last the host stored.
pub fn crash_and_recover(
    storage: &SimStorage,
    config: MetadataConfig,
    snapshot: Option<MetadataSnapshot>,
) -> TestResult<Group> {
    storage.power_loss();
    recover(storage, config, snapshot)
}

/// Makes every command appended to `group` durable.
pub fn sync(group: &mut Group) -> TestResult {
    let _durable = block_on(group.sync())??;
    Ok(())
}

/// The partition `partition`.
pub fn pid(partition: u32) -> TestResult<PartitionId> {
    Ok(PartitionId::new(partition)?)
}

/// The epoch `epoch`.
pub fn epoch(epoch: u64) -> TestResult<Epoch> {
    Ok(Epoch::new(epoch)?)
}

/// The bucket range `start..end`.
pub fn range(start: u32, end: u32) -> TestResult<BucketRange> {
    Ok(BucketRange::new(start, end)?)
}

/// The namespace `ns` of the scenarios, number 7.
pub fn ns() -> Namespace {
    Namespace::new(7)
}

/// The node every partition is led by.
pub fn node() -> NodeId {
    NodeId::new(1)
}

/// The verifier of `password`, salted with 16 bytes of `salt_byte`, at the least iteration count, which keeps the
/// tests fast.
pub fn verifier(password: &str, salt_byte: u8) -> TestResult<ScramVerifier> {
    Ok(ScramVerifier::derive(
        password,
        &[salt_byte; 16],
        MIN_ITERATIONS,
    )?)
}

/// Closes epochs through `through` in the group's view: learns every close from the one after its last learned,
/// announcing what it holds pending at each and carrying its pending evictions, synced after each.
pub fn learn_through(group: &mut Group, through: u64) -> TestResult {
    let from = group.state().last_learned().get() + 1;
    for close in from..=through {
        let versions = group.state().pending_versions();
        let evictions = group.state().pending_evictions();
        let _learned = group.learn_close(epoch(close)?, &versions, 0, &evictions)?;
        sync(group)?;
    }
    Ok(())
}
