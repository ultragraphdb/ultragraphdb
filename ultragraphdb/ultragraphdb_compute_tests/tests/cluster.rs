/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Real storage processes over played system groups, and a compute session served in process beside one, started once per test
//! process: a scenario isolates itself by a tenant of its own.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use service_utils::ServiceUtil;
use ultragraphdb_common_compute::{ReadTimes, read_times_of};
use ultragraphdb_tests::{
    STORAGE_BINARY, answering, not_hosting, role, scratch, storage_holding, with,
};
use ultragraphdb_tool_bdd::{TestResult, port};

use crate::played_system::{EpochClock, Played, ServedCompute, play};
use crate::tap::{TapLog, tap};

/// The offset above the target's port base of the first cluster's base, and how many ports each cluster takes from its base:
/// the played system groups at the base and one above, the partitions from 10, the taps from 30, and from 18 to 22 the
/// observability of the cores the steps build, the compute session's address and storage's observability.
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

    /// Where compute listens.
    pub fn compute_address(&self) -> String {
        format!("tcp://127.0.0.1:{}", self.base + 20)
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
    let played = |listed, compute| Played {
        partitions,
        clock: Arc::clone(&clock),
        listed,
        compute,
    };
    play(base, played(real, None))?;
    play(base + 1, played(tapped, Some(served_compute(base, extra)?)))?;
    for partition in 1..=partitions {
        tap(
            base + 30 + partition - 1,
            partition,
            info.partition_port(partition),
            &log,
        )?;
    }
    let storage = start_storage(util, &info, extra)?;
    Ok((info, vec![storage]))
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

/// The compute session a cluster's tapped group serves at its base's port 20, its reads stalling and retaining as `extra`'s
/// `storage.scan_stall_ms` and `storage.read_retain_ms` say, as the compute binary read them from its settings.
pub fn served_compute(base: u32, extra: &[(&str, &str)]) -> TestResult<ServedCompute> {
    let setting = |name: &str| -> TestResult<Option<u64>> {
        extra
            .iter()
            .find(|(held, _)| *held == name)
            .map(|(_, value)| value.parse::<u64>())
            .transpose()
            .map_err(Into::into)
    };
    let times: ReadTimes = read_times_of(
        setting("storage.scan_stall_ms")?,
        setting("storage.read_retain_ms")?,
    );
    Ok(ServedCompute {
        port: base + 20,
        times,
    })
}

/// A tenant no earlier scenario wrote.
pub fn fresh_tenant() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
