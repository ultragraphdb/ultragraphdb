/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Hosts of partitions over simulated stores and logs, exchanging on one thread through a mailbox mesh.

use ultragraphdb_cluster_partition::{PartitionHost, PartitionMailbox, PartitionOutcome};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_sim::{SimListCache, SimStore, SteppedExecutor};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};

use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};

use crate::utils::cluster::{Log, block_on, epoch, node};

/// A host of the tests.
pub type Host<'a> =
    PartitionHost<'a, Log, SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

/// A fresh write-ahead log over simulated storage.
pub fn sim_log(id: PartitionId) -> TestResult<Log> {
    let storage = SimStorage::new(u64::from(id.get()));
    let (log, _) =
        block_on(Wal::open(storage, "/wal", WalConfig::new(1, true)))?.context("the log opens")?;
    Ok(log)
}

/// Runs `body` on hosts of `count` partitions over the even map, partition `i + 1` at position `i`, epoch 1 closed.
pub fn on_hosts(count: u32, body: impl FnOnce(&mut Vec<Host<'_>>) -> TestResult) -> TestResult {
    let placement = PlacementMap::even(count, NodeId::new(1))?;
    let first = epoch(1)?;
    let ids = (1..=count)
        .map(PartitionId::new)
        .collect::<Result<Vec<_>, _>>()?;
    let parts = ids
        .iter()
        .map(|id| node(*id, &placement, first))
        .collect::<TestResult<Vec<_>>>()?;
    let mut mailboxes = PartitionMailbox::mesh(&ids).into_iter();
    let mut hosts = Vec::new();
    for part in &parts {
        let log = sim_log(part.id())?;
        let pipeline = Pipeline::new(part.applier(), log, RoundBounds::new(u32::MAX, u32::MAX))?
            .with_partition(part.id().get(), first, CloseRequest::default(), &[])
            .bound(part.id(), part.placement());
        let mailbox = mailboxes.next().context("a mailbox per partition")?;
        hosts.push(PartitionHost::new(part, pipeline, mailbox));
    }
    body(&mut hosts)
}

/// Turns every host `turns` times at `now_us`, in order, and gives what ended.
pub fn turn_all(
    hosts: &mut [Host<'_>],
    turns: usize,
    now_us: u64,
) -> TestResult<Vec<PartitionOutcome>> {
    let mut ended = Vec::new();
    for _ in 0..turns {
        for host in hosts.iter_mut() {
            block_on(host.turn(now_us))??;
            ended.extend(host.take_outcomes());
        }
    }
    Ok(ended)
}
