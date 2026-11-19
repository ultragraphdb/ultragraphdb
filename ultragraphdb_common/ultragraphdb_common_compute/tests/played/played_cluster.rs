/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A cluster on the test's one runtime: a played system group, played partitions, and one [`ComputeCore`] whose reads a
//! client's session hands to its executor.

use std::future::Future;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_compute::{ComputeCore, ReadExecutor, ReadTimes};
use ultragraphdb_common_role::{RoleCluster, RoleSecret};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::apart::apart_cluster::links_listing;
use crate::apart::apart_session::ReadSession;

use super::played_partition::PlayedPartition;
use super::played_system::{PlayedSystem, even_ranges};

/// The played cluster's identity.
pub const CLUSTER: u64 = 7;

/// The secret files written, so each test writes its own.
static SECRETS: AtomicU64 = AtomicU64::new(0);

/// The role secret the played cluster's roles hold.
pub const SECRET: &str = "the played cluster's role secret";

/// A role secret file of the test's own, holding `secret`.
fn secret_file(secret: &str) -> TestResult<String> {
    let written = SECRETS.fetch_add(1, Ordering::Relaxed);
    let file = std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!(
            "compute-played-secret-{}-{written}",
            std::process::id()
        ));
    std::fs::write(&file, secret)?;
    Ok(file.display().to_string())
}

/// The played cluster as a role holding `secret` sees it, its deadline `deadline`.
pub fn role_cluster(secret: &str, deadline: Duration) -> TestResult<RoleCluster> {
    Ok(RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&secret_file(secret)?)?,
        deadline,
    ))
}

/// The times of a read that holds no version a test does not ask for: a stall of 2 s, so a peer is hung past 4 s, and a
/// retention of 60 s, so a read choosing its version as it begins holds it from 15 s on.
pub fn quiet_times() -> ReadTimes {
    ReadTimes::new(Duration::from_secs(2), Duration::from_secs(60))
}

/// How long one test's work may take in all.
const TEST_WAIT: Duration = Duration::from_secs(60);

/// Runs `work` on a runtime of its own, failing past [`TEST_WAIT`].
pub fn run<T>(work: impl Future<Output = TestResult<T>>) -> TestResult<T> {
    Runtime::new()?.block_on(async {
        compio::time::timeout(TEST_WAIT, work).await.map_err(|_| {
            TestError::new(format!("the test's work did not end within {TEST_WAIT:?}"))
        })?
    })
}

/// A played cluster.
#[derive(Debug)]
pub struct PlayedCluster {
    system: PlayedSystem,
    partitions: Vec<PlayedPartition>,
    core: Rc<ComputeCore>,
}

impl PlayedCluster {
    /// A cluster of `partitions` partitions placed evenly, compute reading by `times` over its core's links (design D2).
    pub async fn start(partitions: u32, times: ReadTimes) -> TestResult<Self> {
        Self::start_placed(even_ranges(partitions)?, partitions, times).await
    }

    /// A cluster of `partitions` partitions whose system group answers the placement `ranges`, compute reading by `times` over
    /// its core's links.
    pub async fn start_placed(
        ranges: Vec<(u32, u32, u32)>,
        partitions: u32,
        times: ReadTimes,
    ) -> TestResult<Self> {
        Self::started(ranges, partitions, times).await
    }

    /// The cluster over the placement `ranges`, compute reading by `times` over its core's links.
    async fn started(
        ranges: Vec<(u32, u32, u32)>,
        partitions: u32,
        times: ReadTimes,
    ) -> TestResult<Self> {
        let cluster = role_cluster(SECRET, Duration::from_secs(5))?;
        let system = PlayedSystem::start(&cluster, ranges).await?;
        let mut played = Vec::new();
        let mut listed = Vec::new();
        for partition in 1..=partitions {
            let started = PlayedPartition::start(&cluster, partition).await?;
            system.list(partition, started.address());
            listed.push((partition, started.address().to_string()));
            played.push(started);
        }
        let group = Address::parse(system.address())?;
        let map = PlacementMap::even(partitions, NodeId::new(1))?;
        let core = ComputeCore::over(links_listing(&cluster, map, &listed), group, times);
        Ok(Self {
            system,
            partitions: played,
            core,
        })
    }

    /// The played system group.
    pub fn system(&self) -> &PlayedSystem {
        &self.system
    }

    /// Played partition `partition`.
    pub fn partition(&self, partition: u32) -> TestResult<&PlayedPartition> {
        self.partitions
            .iter()
            .find(|played| played.partition() == partition)
            .context("a played partition")
    }

    /// A client connection's reads on compute's core, as the connection's session runs them.
    pub fn client(&self) -> ReadSession {
        ReadSession::new(CLUSTER, ReadExecutor::new(Rc::clone(&self.core)))
    }
}
