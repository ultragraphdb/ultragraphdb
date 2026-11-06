/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A real compute process and real storage processes over played frontends, started once per test process: a scenario isolates
//! itself by a tenant of its own.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use service_utils::ServiceUtil;
use ultragraphdb_tests::{
    COMPUTE_BINARY, SECRET, STORAGE_BINARY, answering, compute_on_one_host, holding, role, scratch,
    storage_holding, with,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::played_frontend::{EpochClock, Played, play};
use crate::tap::{TapLog, tap};

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

    /// Where compute listens.
    pub fn compute_address(&self) -> String {
        format!("tcp://127.0.0.1:{}", self.base + 50)
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
            format!("tcp://127.0.0.1:{}", base + 52),
        ),
        (
            "cluster.frontend_address",
            format!("tcp://127.0.0.1:{base}"),
        ),
        (
            "storage.role_address",
            format!("tcp://127.0.0.1:{}", base + 10),
        ),
        ("storage.partition_count", info.partitions.to_string()),
    ];
    let mut held = storage_holding()?;
    for (name, value) in &settings {
        held = with(held, name, value);
    }
    for (name, value) in extra {
        held = with(held, name, value);
    }
    let wait = answering(&format!("127.0.0.1:{}", base + 52), "/ready");
    Ok(util.start_service_from_config(role(
        STORAGE_BINARY,
        &held,
        &scratch(&format!("storage-{base}.log")),
        wait,
    ))?)
}

pub fn start_compute(util: &ServiceUtil, base: u32, extra: &[(&str, &str)]) -> TestResult<u32> {
    let settings = [
        (
            "observability.address",
            format!("tcp://127.0.0.1:{}", base + 51),
        ),
        (
            "cluster.frontend_address",
            format!("tcp://127.0.0.1:{}", base + 1),
        ),
        (
            "compute.role_address",
            format!("tcp://127.0.0.1:{}", base + 50),
        ),
    ];
    let mut held = holding(compute_on_one_host(), SECRET)?;
    for (name, value) in &settings {
        held = with(held, name, value);
    }
    for (name, value) in extra {
        held = with(held, name, value);
    }
    let wait = answering(&format!("127.0.0.1:{}", base + 51), "/ready");
    Ok(util.start_service_from_config(role(
        COMPUTE_BINARY,
        &held,
        &scratch(&format!("compute-{base}.log")),
        wait,
    ))?)
}

/// A tenant no earlier scenario wrote.
pub fn fresh_tenant() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
