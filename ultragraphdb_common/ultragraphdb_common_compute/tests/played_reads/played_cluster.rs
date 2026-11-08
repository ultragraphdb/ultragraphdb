/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A cluster on the test's own runtime: a played frontend, played partitions and the crate's compute core serving its role address,
//! each over a TCP port of its own, every connection authenticated as the roles authenticate.

use std::collections::BTreeMap;
use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_compute::{ComputeCore, ComputeSession, ReadTimes};
use ultragraphdb_common_role::{
    COMPUTE_WELCOME_CREDIT, PARTITION_INITIAL_CREDIT, ROLE_ITERATIONS, RoleClient, RoleCluster,
    RoleCredentials, RoleSecret, Server, Session, serve_connection,
};
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::Welcome;
use ultragraphdb_tool_bdd::{TestError, TestResult};

use super::played_frontend::FrontendConnection;
use super::played_partition::{PartitionConnection, PlayedPartition};

/// The cluster every played role serves.
pub const CLUSTER: u64 = 7;

/// The role secret every played role holds.
const PASSWORD: &str = "a played role secret";

/// The secret files written so far by this process, each test's a file of its own.
static SECRETS: AtomicU64 = AtomicU64::new(0);

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

/// A cluster of played partitions and a compute core of this crate.
#[derive(Debug)]
pub struct PlayedCluster {
    /// The partitions, partition 1 first.
    pub partitions: Vec<Rc<PlayedPartition>>,
    /// The compute core, as the query core calls its lookups.
    pub core: Rc<ComputeCore>,
    compute: Address,
}

/// What every played role authenticates its peers against, and the cluster's settings, read from a file of the test's.
fn secret() -> TestResult<(RoleSecret, RoleCredentials)> {
    let file = std::env::temp_dir().join(format!(
        "common-compute-played-secret-{}-{}",
        std::process::id(),
        SECRETS.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&file, PASSWORD)?;
    let secret = RoleSecret::read(&file.display().to_string())?;
    let credentials = secret.credentials()?;
    Ok((secret, credentials))
}

/// A listener at a port of its own on this host, and its address.
async fn listener() -> TestResult<(Listener, Address)> {
    let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
    let address = listener.local_address()?;
    Ok((listener, address))
}

/// The Welcome of each played role's endpoint: a partition's and compute's stream credit as their services state it.
fn welcome_of(endpoint: Endpoint) -> fn(Welcome) -> Welcome {
    match endpoint {
        Endpoint::Partition => |welcome| welcome.with_initial_credit(PARTITION_INITIAL_CREDIT),
        Endpoint::Compute => |welcome| welcome.with_initial_credit(COMPUTE_WELCOME_CREDIT),
        Endpoint::Client | Endpoint::Frontend => |welcome| welcome,
    }
}

/// Serves every connection `listener` accepts as `endpoint`, each with the session `session` makes.
fn serve<S, F>(
    listener: Listener,
    endpoint: Endpoint,
    credentials: RoleCredentials,
    session: F,
) -> TestResult
where
    S: Session + 'static,
    F: Fn() -> S + 'static,
{
    let server = Server::new(CLUSTER, "played", endpoint)?
        .with_iterations(ROLE_ITERATIONS)
        .with_welcome(welcome_of(endpoint));
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, credentials, session) = (server.clone(), credentials.clone(), session());
            spawn(async move { serve_connection(carrier, &server, credentials, session).await })
                .detach();
        }
    })
    .detach();
    Ok(())
}

impl PlayedCluster {
    /// `partitions` played partitions placed evenly, and a compute core reading by `times`.
    pub async fn start(partitions: u32, times: ReadTimes) -> TestResult<Self> {
        let (secret, credentials) = secret()?;
        let mut played = Vec::new();
        let mut listed = Vec::new();
        for partition in 1..=partitions {
            let (listener, address) = listener().await?;
            let at = PlayedPartition::new(partition);
            listed.push((partition, address.to_string()));
            let serving = Rc::clone(&at);
            serve(
                listener,
                Endpoint::Partition,
                credentials.clone(),
                move || {
                    serving.connected();
                    PartitionConnection {
                        played: Rc::clone(&serving),
                        cluster: CLUSTER,
                        pushes: BTreeMap::new(),
                    }
                },
            )?;
            played.push(at);
        }
        let (frontend_listener, frontend) = listener().await?;
        serve(
            frontend_listener,
            Endpoint::Frontend,
            credentials.clone(),
            move || FrontendConnection {
                cluster: CLUSTER,
                partitions,
                listed: listed.clone(),
            },
        )?;
        let cluster = RoleCluster::new(CLUSTER, secret, Duration::from_secs(5));
        let core = ComputeCore::new(cluster, frontend, times, "compute");
        let (compute_listener, compute) = listener().await?;
        let serving = Rc::clone(&core);
        serve(
            compute_listener,
            Endpoint::Compute,
            credentials,
            move || ComputeSession::new(Rc::clone(&serving)),
        )?;
        Ok(Self {
            partitions: played,
            core,
            compute,
        })
    }

    /// Partition `partition`.
    pub fn partition(&self, partition: u32) -> TestResult<&Rc<PlayedPartition>> {
        self.partitions
            .get(partition.saturating_sub(1) as usize)
            .ok_or_else(|| TestError::new(format!("no partition {partition}")))
    }

    /// A connection to compute's role address, as the frontend's.
    pub async fn client(&self) -> TestResult<RoleClient> {
        Ok(RoleClient::dial(&self.compute, CLUSTER, "frontend", PASSWORD).await?)
    }
}
