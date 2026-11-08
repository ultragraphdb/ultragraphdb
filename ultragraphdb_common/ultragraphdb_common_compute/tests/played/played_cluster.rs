/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A cluster on the test's one runtime: a played frontend, played partitions, and one [`ComputeCore`] whose
//! [`ComputeSession`]s serve at an address of their own, which the test dials as the frontend does.

use std::future::Future;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_compute::{ComputeCore, ComputeSession, ReadTimes};
use ultragraphdb_common_role::{
    COMPUTE_WELCOME_CREDIT, ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server,
    serve_connection,
};
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use super::played_frontend::{PlayedFrontend, even_ranges};
use super::played_partition::PlayedPartition;

/// The played cluster's identity.
pub const CLUSTER: u64 = 7;

/// The secret files written, so each test writes its own.
static SECRETS: AtomicU64 = AtomicU64::new(0);

/// A role secret file of the test's own.
fn secret_file() -> TestResult<String> {
    let written = SECRETS.fetch_add(1, Ordering::Relaxed);
    let file = std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!(
            "compute-played-secret-{}-{written}",
            std::process::id()
        ));
    std::fs::write(&file, "the played cluster's role secret")?;
    Ok(file.display().to_string())
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
    cluster: RoleCluster,
    frontend: PlayedFrontend,
    partitions: Vec<PlayedPartition>,
    compute: String,
}

impl PlayedCluster {
    /// A cluster of `partitions` partitions placed evenly, compute reading by `times`.
    pub async fn start(partitions: u32, times: ReadTimes) -> TestResult<Self> {
        Self::start_placed(even_ranges(partitions)?, partitions, times).await
    }

    /// A cluster of `partitions` partitions whose frontend answers the placement `ranges`, compute reading by `times`.
    pub async fn start_placed(
        ranges: Vec<(u32, u32, u32)>,
        partitions: u32,
        times: ReadTimes,
    ) -> TestResult<Self> {
        let cluster = RoleCluster::new(
            CLUSTER,
            RoleSecret::read(&secret_file()?)?,
            Duration::from_secs(5),
        );
        let frontend = PlayedFrontend::start(&cluster, ranges).await?;
        let mut played = Vec::new();
        for partition in 1..=partitions {
            let started = PlayedPartition::start(&cluster, partition).await?;
            frontend.list(partition, started.address());
            played.push(started);
        }
        let core = ComputeCore::new(
            cluster.clone(),
            Address::parse(frontend.address())?,
            times,
            "compute",
        );
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let compute = listener.local_address()?.to_string();
        let credentials = cluster.secret().credentials()?;
        let server = Server::new(cluster.id(), "role", Endpoint::Compute)?
            .with_welcome(|welcome| welcome.with_initial_credit(COMPUTE_WELCOME_CREDIT))
            .with_iterations(ROLE_ITERATIONS);
        spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let session = ComputeSession::new(Rc::clone(&core));
                let (server, credentials) = (server.clone(), credentials.clone());
                spawn(
                    async move { serve_connection(carrier, &server, credentials, session).await },
                )
                .detach();
            }
        })
        .detach();
        Ok(Self {
            cluster,
            frontend,
            partitions: played,
            compute,
        })
    }

    /// The played frontend.
    pub fn frontend(&self) -> &PlayedFrontend {
        &self.frontend
    }

    /// Played partition `partition`.
    pub fn partition(&self, partition: u32) -> TestResult<&PlayedPartition> {
        self.partitions
            .iter()
            .find(|played| played.partition() == partition)
            .context("a played partition")
    }

    /// A connection to compute, as the frontend dials one.
    pub async fn client(&self) -> TestResult<RoleClient> {
        let address = Address::parse(&self.compute)?;
        Ok(RoleClient::dial(
            &address,
            self.cluster.id(),
            "frontend",
            self.cluster.secret().password(),
        )
        .await?)
    }
}
