/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use futures::channel::mpsc::unbounded;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{
    JoinedRoles, OwnerHandle, OwnerStart, OwnerThread, RoleLinks, RoleSession, RoleShared,
    start_owner,
};
use ultragraphdb_common_role::{
    ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server, listen, serve_connection,
};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::Join;
use ultragraphdb_store_sim::LogCapture;
use ultragraphdb_tool_bdd::{TestError, TestResult};

const CLUSTER: u64 = 7;

fn directory(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("joined-roles-{name}-{}", std::process::id()))
}

/// The role cluster whose secret is in a file under `data`.
fn cluster_under(data: &std::path::Path) -> TestResult<RoleCluster> {
    let file = data.join("role-secret");
    std::fs::write(&file, "a role secret")?;
    let secret = RoleSecret::read(&file.display().to_string())?;
    Ok(RoleCluster::new(CLUSTER, secret, Duration::from_secs(1)))
}

/// Serves the role connections `listener` accepts as `endpoint`, each by a session over `shared`.
fn serve_roles(
    listener: Listener,
    endpoint: Endpoint,
    shared: RoleShared,
    cluster: &RoleCluster,
) -> TestResult {
    let server = Server::new(CLUSTER, "frontend", endpoint)?.with_iterations(ROLE_ITERATIONS);
    let credentials = cluster.secret().credentials()?;
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, credentials) = (server.clone(), credentials.clone());
            let session = RoleSession::new(shared.clone());
            spawn(async move { serve_connection(carrier, &server, credentials, session).await })
                .detach();
        }
    })
    .detach();
    Ok(())
}

/// A free port on the loopback address, which nothing listens on until a test binds it.
fn free_port() -> TestResult<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

/// A role endpoint over `owner` of `cluster`, each join its sessions register taken by the frontend's task, ready as
/// `readiness` says: its address.
async fn role_endpoint(
    owner: OwnerHandle,
    cluster: &RoleCluster,
    readiness: Readiness,
) -> TestResult<Address> {
    let links = RoleLinks::new(cluster.clone(), "frontend".to_string());
    let (joined, joins) = JoinedRoles::new(cluster.clone(), links, readiness);
    spawn(joined.follow()).detach();
    let shared = RoleShared::new(cluster.clone(), owner, joins);
    let listener = listen(&format!("tcp://127.0.0.1:{}", free_port()?)).await?;
    let address = listener.local_address()?;
    serve_roles(listener, Endpoint::Frontend, shared, cluster)?;
    Ok(address)
}

/// An owner over `data`, closing an epoch every 1 to 50 ms.
async fn owner_under(data: &std::path::Path) -> TestResult<OwnerThread> {
    if data.exists() {
        std::fs::remove_dir_all(data)?;
    }
    std::fs::create_dir_all(data)?;
    let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
    Ok(start_owner(
        ServiceConfig::default(),
        CLUSTER,
        data.to_path_buf(),
        start,
        Vec::new(),
    )
    .await?)
}

/// A storage process joining is connected back to, the frontend retrying while its address refuses, and the first partition
/// reached makes the frontend ready.
#[test]
fn test_a_storage_join_is_connected_back_and_makes_the_frontend_ready() -> TestResult {
    let data = directory("connect-back");
    Runtime::new()?.block_on(async {
        let owner = owner_under(&data).await?;
        let handle = owner.handle().clone();
        let cluster = cluster_under(&data)?;
        let readiness = Readiness::default();
        let address = role_endpoint(handle.clone(), &cluster, readiness.clone()).await?;
        let password = cluster.secret().password();
        let mut storage = RoleClient::dial(&address, CLUSTER, "storage", password).await?;
        let port = free_port()?;
        let joined = format!("tcp://127.0.0.1:{port}");
        drop(
            storage
                .request(&Join::storage(joined.clone(), vec![1]))
                .await?,
        );
        // Two of the retries refused before the address listens, well inside the deadline.
        compio::time::sleep(Duration::from_millis(250)).await;
        assert!(!readiness.is_ready(), "not ready while the address refuses");
        let (joins, _joined) = unbounded();
        let shared = RoleShared::new(cluster.clone(), handle, joins);
        serve_roles(
            listen(&joined).await?,
            Endpoint::Partition,
            shared,
            &cluster,
        )?;
        for _ in 0..5_000 {
            if readiness.is_ready() {
                return Ok(());
            }
            compio::time::sleep(Duration::from_millis(1)).await;
        }
        Err(TestError::new("the frontend did not become ready"))
    })
}

/// A joined role whose address refuses past the deadline is given up, and the log says so.
#[test]
fn test_a_role_refusing_past_the_deadline_is_given_up() -> TestResult {
    let data = directory("given-up");
    let capture = LogCapture::new();
    let readiness = Readiness::default();
    capture.capture(|| -> TestResult {
        Runtime::new()?.block_on(async {
            let owner = owner_under(&data).await?;
            let cluster = cluster_under(&data)?;
            let address =
                role_endpoint(owner.handle().clone(), &cluster, readiness.clone()).await?;
            let password = cluster.secret().password();
            let mut storage = RoleClient::dial(&address, CLUSTER, "storage", password).await?;
            let joined = format!("tcp://127.0.0.1:{}", free_port()?);
            drop(storage.request(&Join::storage(joined, vec![1])).await?);
            let given_up = || {
                capture
                    .events()
                    .iter()
                    .any(|event| event.to_string().contains("could not connect back"))
            };
            for _ in 0..5_000 {
                if given_up() {
                    return Ok(());
                }
                compio::time::sleep(Duration::from_millis(1)).await;
            }
            Err(TestError::new("the frontend did not give up"))
        })
    })?;
    assert!(!readiness.is_ready());
    Ok(())
}

/// A Join at an address that does not parse reaches the links and the log, and nothing connects back to it.
#[test]
fn test_a_join_at_an_unparsed_address_is_not_connected_back() -> TestResult {
    let data = directory("unparsed");
    std::fs::create_dir_all(&data)?;
    let capture = LogCapture::new();
    let readiness = Readiness::default();
    capture.capture(|| -> TestResult {
        Runtime::new()?.block_on(async {
            let cluster = cluster_under(&data)?;
            let links = RoleLinks::new(cluster.clone(), "frontend".to_string());
            let (joined, joins) = JoinedRoles::new(cluster, links.clone(), readiness.clone());
            joins.unbounded_send(Join::compute("tcp://a host with no port"))?;
            drop(joins);
            joined.follow().await;
            compio::time::sleep(Duration::from_millis(10)).await;
            assert_eq!(links.compute_count(), 1, "the links hold it");
            Ok(())
        })
    })?;
    let warned = capture
        .events()
        .iter()
        .any(|event| event.to_string().contains("does not parse"));
    assert!(warned, "the log names it");
    assert!(!readiness.is_ready());
    Ok(())
}
