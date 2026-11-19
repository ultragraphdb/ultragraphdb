/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Real storage and compute processes over played system groups, started once per test process: a scenario isolates itself by a
//! tenant of its own, and reads through compute's client endpoint.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use service_utils::ServiceUtil;
use ultragraphdb_tests::{
    COMPUTE_BINARY, STORAGE_BINARY, answering, compute_holding, not_hosting, role, scratch,
    storage_holding, with,
};
use ultragraphdb_tool_bdd::{TestResult, port};

use crate::played_system::{EpochClock, Played, play};
use crate::tap::{TapLog, tap};

/// The offset above the target's port base of the first cluster's base, and how many ports each cluster takes from its base:
/// the played system groups at the base and one above, the partitions from 10, the taps from 30, and from 18 to 22 the
/// observability of the cores the steps build, compute's client endpoint, compute's observability and storage's.
const FIRST_CLUSTER: u16 = 20;
const CLUSTER_PORTS: u16 = 34;

/// The base of the cluster `at`, the first 0, each a block of [`CLUSTER_PORTS`] above the one before.
pub fn cluster_base(at: u16) -> TestResult<u32> {
    Ok(u32::from(port(FIRST_CLUSTER + at * CLUSTER_PORTS)?))
}

/// What a scenario reads of a cluster.
#[derive(Debug)]
pub struct ClusterInfo {
    pub partitions: u32,
    pub base: u32,
    pub clock: Arc<EpochClock>,
    pub log: Arc<TapLog>,
}

impl ClusterInfo {
    /// Where `partition` really listens.
    pub fn partition_port(&self, partition: u32) -> u32 {
        self.base + 10 + partition - 1
    }

    /// Where compute's client endpoint listens.
    pub fn compute_address(&self) -> String {
        format!("tcp://127.0.0.1:{}", self.base + 20)
    }

    /// The log of the cluster's compute process.
    pub fn compute_log(&self) -> std::path::PathBuf {
        scratch(&format!("compute-{}.log", self.base))
    }
}

/// A cluster over `partitions` partitions at the ports from `base`, its storage and compute processes holding `extra` beside
/// their own settings: their pids.
pub fn start_cluster(
    util: &ServiceUtil,
    base: u32,
    partitions: u32,
    extra: &[(&str, &str)],
) -> TestResult<(ClusterInfo, Vec<u32>)> {
    let clock = EpochClock::new();
    let log = Arc::new(TapLog::default());
    let info = ClusterInfo {
        partitions,
        base,
        clock: Arc::clone(&clock),
        log: Arc::clone(&log),
    };
    let real: Vec<(u32, String)> = (1..=partitions)
        .map(|partition| {
            (
                partition,
                format!("tcp://127.0.0.1:{}", info.partition_port(partition)),
            )
        })
        .collect();
    let tapped: Vec<(u32, String)> = (1..=partitions)
        .map(|partition| {
            (
                partition,
                format!("tcp://127.0.0.1:{}", base + 30 + partition - 1),
            )
        })
        .collect();
    let played = |listed| Played {
        partitions,
        clock: Arc::clone(&clock),
        listed,
    };
    play(base, played(real))?;
    play(base + 1, played(tapped))?;
    for partition in 1..=partitions {
        tap(
            base + 30 + partition - 1,
            partition,
            info.partition_port(partition),
            &log,
        )?;
    }
    let storage = start_storage(util, &info, extra)?;
    let compute = start_compute(util, base, extra)?;
    Ok((info, vec![compute, storage]))
}

fn start_storage(
    util: &ServiceUtil,
    info: &ClusterInfo,
    extra: &[(&str, &str)],
) -> TestResult<u32> {
    let base = info.base;
    let settings = [
        (
            "observability.address",
            format!("tcp://127.0.0.1:{}", base + 22),
        ),
        ("cluster.system_address", format!("tcp://127.0.0.1:{base}")),
        (
            "storage.role_address",
            format!("tcp://127.0.0.1:{}", base + 10),
        ),
        ("storage.partition_count", info.partitions.to_string()),
    ];
    // The played frontend plays the system group, so the storage process hosts none.
    let mut held = not_hosting(storage_holding()?);
    for (name, value) in &settings {
        held = with(held, name, value);
    }
    for (name, value) in extra {
        held = with(held, name, value);
    }
    let wait = answering(&format!("127.0.0.1:{}", base + 22), "/ready");
    Ok(util.start_service_from_config(role(
        STORAGE_BINARY,
        &held,
        &scratch(&format!("storage-{base}.log")),
        wait,
    )?)?)
}

/// The compute process of the cluster at `base`: its client endpoint at the base's port 20 in clear text, its observability at
/// 21, dialling the tapped group at the base's port 1, its settings holding `extra` beside its own. Its pid, once it is ready.
pub fn start_compute(util: &ServiceUtil, base: u32, extra: &[(&str, &str)]) -> TestResult<u32> {
    let settings = [
        (
            "observability.address",
            format!("tcp://127.0.0.1:{}", base + 21),
        ),
        (
            "cluster.system_address",
            format!("tcp://127.0.0.1:{}", base + 1),
        ),
        ("endpoint.address", format!("tcp://127.0.0.1:{}", base + 20)),
    ];
    let mut held = compute_holding()?;
    for (name, value) in &settings {
        held = with(held, name, value);
    }
    for (name, value) in extra {
        held = with(held, name, value);
    }
    let wait = answering(&format!("127.0.0.1:{}", base + 21), "/ready");
    Ok(util.start_service_from_config(role(
        COMPUTE_BINARY,
        &held,
        &scratch(&format!("compute-{base}.log")),
        wait,
    )?)?)
}

/// A tenant no earlier scenario wrote.
pub fn fresh_tenant() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
