/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{
    OwnerHandle, OwnerStart, RoleLinks, RoleSession, RoleShared, start_owner,
};
use ultragraphdb_common_role::{
    ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server, listen, serve_connection,
};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportError, TransportErrorEnum};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyView, Cancel, Catalog, CloseRequest, Closed, Command, Declared, DeclaredPredicate,
    EpochCloseView, ErrorCode, ErrorReplyView, Join, Lease, Members, MembersView, Placement,
    Report, ReportView, RequestTerm, Subscribe,
};
use ultragraphdb_store_sim::LogCapture;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{FaultMode, SimStorage};

use super::owner::{EPOCH_US, TestClock, ended, recovered, running, starting};

const CLUSTER: u64 = 7;
const PREDICATE: &str = "http://ex.org/p";

fn directory(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("role-session-{name}-{}", std::process::id()))
}

/// A subscription whose next close the sequencer no longer holds is ended with Error 1 on its stream, so its storage process
/// learns it is behind instead of waiting on a stream that carries nothing again (`PROTOCOL.md` §13.3).
#[test]
fn test_a_subscription_past_the_closes_held_ends_with_an_error() -> TestResult {
    let data = directory("trimmed");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    std::fs::create_dir_all(&data)?;
    Runtime::new()?.block_on(async {
        let retained = NonZeroU32::new(2).context("two closes")?;
        let sequencer = SequencerConfig::new(1, 5)?.with_closes_retained(retained);
        let start = OwnerStart::new(MetadataConfig::new(), sequencer, 1);
        let owner = start_owner(
            ServiceConfig::default(),
            CLUSTER,
            data.clone(),
            start,
            Vec::new(),
        )
        .await?;
        let handle = owner.handle().clone();
        // Epochs close every 1 to 5 ms and two closes are held, so under load the sequencer may trim past a cursor before it
        // is asked: a close after epoch 1 or later refused with code 1 is the trim this waits for, and one after epoch 0
        // moves the cursor to epoch 1 to ask again.
        let mut after = 0;
        while after < 8 {
            match handle.next_close(after).await {
                Ok(close) => after = close.epoch().get(),
                Err(refusal) if refusal.code() == ErrorCode::Protocol && after > 0 => break,
                Err(refusal) if refusal.code() == ErrorCode::Protocol => after = 1,
                Err(refusal) => return Err(TestError::new(refusal.message().to_string())),
            }
        }
        let file = data.join("role-secret");
        std::fs::write(&file, "a role secret")?;
        let secret = RoleSecret::read(&file.display().to_string())?;
        let cluster = RoleCluster::new(CLUSTER, secret.clone(), Duration::from_secs(1));
        let links = RoleLinks::new(cluster.clone(), "frontend".to_string());
        let shared = RoleShared::new(cluster, handle, links, Readiness::default());
        let server =
            Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
        let credentials = secret.credentials()?;
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
        let address = listener.local_address()?;
        spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let (server, credentials) = (server.clone(), credentials.clone());
                let session = RoleSession::new(shared.clone());
                spawn(
                    async move { serve_connection(carrier, &server, credentials, session).await },
                )
                .detach();
            }
        })
        .detach();
        let mut storage = RoleClient::dial(&address, CLUSTER, "storage", secret.password()).await?;
        storage.send_only(&Subscribe::new(1)).await?;
        let frame = compio::time::timeout(Duration::from_secs(2), storage.next_frame())
            .await
            .map_err(|_| TestError::new("no frame came on the subscription within 2 s"))??;
        let view = frame.view()?;
        assert_eq!(view.command(), Command::Error);
        assert_eq!(view.header().stream(), storage.last_stream());
        assert_eq!(
            ErrorReplyView::from_frame(&view)?.code(),
            ErrorCode::Protocol
        );
        Ok(())
    })
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

/// A role endpoint over `owner` of `cluster`, ready as `readiness` says: its address.
async fn role_endpoint(
    owner: OwnerHandle,
    cluster: &RoleCluster,
    readiness: Readiness,
) -> TestResult<Address> {
    let links = RoleLinks::new(cluster.clone(), "frontend".to_string());
    let shared = RoleShared::new(cluster.clone(), owner, links, readiness);
    let listener = listen(&format!("tcp://127.0.0.1:{}", free_port()?)).await?;
    let address = listener.local_address()?;
    serve_roles(listener, Endpoint::Frontend, shared, cluster)?;
    Ok(address)
}

/// An owner over `data`, closing an epoch every 1 to 50 ms.
async fn owner_under(
    data: &std::path::Path,
) -> TestResult<ultragraphdb_common_frontend::OwnerThread> {
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

/// A Members request, holding no member itself, is answered with the members the owner holds.
#[test]
fn test_a_members_request_is_answered_with_the_owners_members() -> TestResult {
    let data = directory("members");
    Runtime::new()?.block_on(async {
        let owner = owner_under(&data).await?;
        let handle = owner.handle().clone();
        handle
            .join(Join::compute("tcp://127.0.0.1:1"))
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let held = handle.members().await.context("the members")?;
        let cluster = cluster_under(&data)?;
        let address = role_endpoint(handle, &cluster, Readiness::default()).await?;
        let password = cluster.secret().password();
        let mut compute = RoleClient::dial(&address, CLUSTER, "compute", password).await?;
        let answered = compute.request(&Members::new()).await?;
        let view = answered.view()?;
        assert_eq!(MembersView::from_frame(&view)?.len(), held.len());
        Ok(())
    })
}

/// A subscription takes a close as an EpochClose frame, from the epoch it asked after; a Cancel of its stream ends it, so the
/// connection may subscribe again.
#[test]
fn test_a_subscription_takes_the_closes_and_a_cancel_ends_it() -> TestResult {
    let data = directory("subscribed");
    Runtime::new()?.block_on(async {
        let owner = owner_under(&data).await?;
        let handle = owner.handle().clone();
        let first = handle
            .next_close(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let cluster = cluster_under(&data)?;
        let address = role_endpoint(handle, &cluster, Readiness::default()).await?;
        let password = cluster.secret().password();
        let mut storage = RoleClient::dial(&address, CLUSTER, "storage", password).await?;
        storage.send_only(&Subscribe::new(0)).await?;
        let subscribed = storage.last_stream();
        // A frame holds the connection's one buffer until it is dropped.
        let (on, epoch) = {
            let frame = compio::time::timeout(Duration::from_secs(5), storage.next_frame())
                .await
                .map_err(|_| TestError::new("no close came on the subscription"))??;
            let view = frame.view()?;
            (
                view.header().stream(),
                EpochCloseView::from_frame(&view)?.epoch(),
            )
        };
        assert_eq!((on, epoch), (subscribed, first.epoch().get()));
        storage.send_on(subscribed, &Cancel).await?;
        storage.send_only(&Subscribe::new(0)).await?;
        let again = storage.last_stream();
        loop {
            let frame = compio::time::timeout(Duration::from_secs(5), storage.next_frame())
                .await
                .map_err(|_| TestError::new("nothing came on the second subscription"))??;
            let view = frame.view()?;
            if view.header().stream() == again {
                assert_eq!(view.command(), Command::EpochClose, "subscribed again");
                break;
            }
        }
        Ok(())
    })
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
        let links = RoleLinks::new(cluster.clone(), "frontend".to_string());
        let shared = RoleShared::new(cluster.clone(), handle, links, Readiness::default());
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

/// What a refused request was refused with.
fn refusal_code(refused: &TransportError) -> Option<ErrorCode> {
    if let TransportErrorEnum::Refused { code, .. } = &refused.0 {
        Some(*code)
    } else {
        None
    }
}

/// A session served where a partition's commands arrive hosts none of them: a Closed request there is refused as
/// unavailable.
async fn refused_where_partitions_arrive(cluster: &RoleCluster, handle: OwnerHandle) -> TestResult {
    let links = RoleLinks::new(cluster.clone(), "frontend".to_string());
    let shared = RoleShared::new(cluster.clone(), handle, links, Readiness::default());
    let listener = listen(&format!("tcp://127.0.0.1:{}", free_port()?)).await?;
    let elsewhere = listener.local_address()?;
    serve_roles(listener, Endpoint::Partition, shared, cluster)?;
    let password = cluster.secret().password();
    let mut frontend = RoleClient::dial(&elsewhere, CLUSTER, "frontend", password).await?;
    let refused = frontend
        .request(&Closed::request(&[1]))
        .await
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal_code(&refused),
        Some(ErrorCode::Unavailable),
        "{refused}"
    );
    Ok(())
}

/// Lease, Placement and Catalog are answered from the owner, a CloseRequest is taken with no answer, and the role endpoint
/// refuses a Members request holding members, a second subscription on one connection and a command it does not host.
#[test]
fn test_the_role_commands_are_answered_from_the_owner() -> TestResult {
    let data = directory("commands");
    Runtime::new()?.block_on(async {
        let owner = owner_under(&data).await?;
        let handle = owner.handle().clone();
        let cluster = cluster_under(&data)?;
        let address = role_endpoint(handle.clone(), &cluster, Readiness::default()).await?;
        let password = cluster.secret().password();
        let mut storage = RoleClient::dial(&address, CLUSTER, "storage", password).await?;
        // A frame holds the connection's one buffer until it is dropped, so each answer is read and dropped in turn.
        let leased = storage
            .request(&Lease::request(1, 1, 8))
            .await?
            .view()?
            .command();
        assert_eq!(leased, Command::Lease);
        let placed = storage
            .request(&Placement::request(0))
            .await?
            .view()?
            .command();
        assert_eq!(placed, Command::Placement);
        let _version = handle
            .declare(
                0,
                0,
                vec![Declared::Predicate(DeclaredPredicate::new(PREDICATE))],
            )
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let catalog = storage
            .request(&Catalog::request(0, 0))
            .await?
            .view()?
            .command();
        assert_eq!(catalog, Command::Catalog);
        storage.send_only(&CloseRequest::new(1, 1)).await?;
        // A Join at an address that does not parse is held, and nothing connects back to it.
        let unparsed = Join::compute("tcp://a host with no port");
        let joined = storage.request(&unparsed).await?.view()?.command();
        assert_eq!(joined, Command::Join);
        let mut holding = Members::new();
        holding.push_compute("tcp://127.0.0.1:1");
        let holding_refused = storage.request(&holding).await.err().context("a refusal")?;
        assert_eq!(
            refusal_code(&holding_refused),
            Some(ErrorCode::Protocol),
            "{holding_refused}"
        );
        refused_where_partitions_arrive(&cluster, handle.clone()).await?;
        storage.send_only(&Subscribe::new(0)).await?;
        storage.send_only(&Subscribe::new(0)).await?;
        let again = storage.last_stream();
        for _ in 0..1_000 {
            let frame = compio::time::timeout(Duration::from_secs(5), storage.next_frame())
                .await
                .map_err(|_| TestError::new("nothing came on the second subscription"))??;
            let view = frame.view()?;
            if view.header().stream() == again {
                assert_eq!(view.command(), Command::Error);
                assert_eq!(
                    ErrorReplyView::from_frame(&view)?.code(),
                    ErrorCode::Protocol
                );
                return Ok(());
            }
        }
        Err(TestError::new("the second subscription was not refused"))
    })
}

/// A report of each kind reaches the owner and is answered with its fields: a failed build, and a refusal naming a blank node or
/// a literal at fault.
#[test]
fn test_a_report_of_each_kind_is_answered() -> TestResult {
    let data = directory("reports");
    Runtime::new()?.block_on(async {
        let owner = owner_under(&data).await?;
        let cluster = cluster_under(&data)?;
        let address = role_endpoint(owner.handle().clone(), &cluster, Readiness::default()).await?;
        let password = cluster.secret().password();
        let mut storage = RoleClient::dial(&address, CLUSTER, "storage", password).await?;
        let blank = RequestTerm::BlankNode("b".to_string());
        let literal = RequestTerm::Literal {
            lexical: "a value".to_string(),
            datatype: None,
        };
        let reports = vec![
            Report::build_failed(1, 0, 1, PREDICATE),
            Report::refused(1, 0, 1, PREDICATE, blank)?,
            Report::refused(1, 0, 1, PREDICATE, literal)?,
        ];
        for report in &reports {
            let kind = ReportView::from_frame(&storage.request(report).await?.view()?)?.kind();
            assert_eq!(kind, report.kind());
        }
        Ok(())
    })
}

/// The closes a subscription takes carry the catalog versions they announce and the sessions they evict.
#[test]
fn test_a_subscription_takes_announcements_and_evictions() -> TestResult {
    let data = directory("announced");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    std::fs::create_dir_all(&data)?;
    Runtime::new()?.block_on(async {
        let start = OwnerStart::new(
            MetadataConfig::new().with_sessions_max(1)?,
            SequencerConfig::new(1, 50)?,
            1,
        );
        let verifier = ScramVerifier::derive("pw", b"salt", 4_096)?;
        let owner = start_owner(
            ServiceConfig::default(),
            CLUSTER,
            data.clone(),
            start,
            vec![("ann".to_string(), verifier)],
        )
        .await?;
        let handle = owner.handle().clone();
        let cluster = cluster_under(&data)?;
        let address = role_endpoint(handle.clone(), &cluster, Readiness::default()).await?;
        let password = cluster.secret().password();
        let mut storage = RoleClient::dial(&address, CLUSTER, "storage", password).await?;
        storage.send_only(&Subscribe::new(0)).await?;
        let heard = |refusal: ultragraphdb_protocol_wire::ErrorReply| {
            TestError::new(refusal.message().to_string())
        };
        let ann = handle
            .verifier("ann".to_string(), None)
            .await
            .0
            .context("ann")?
            .0;
        handle.register_session(1, ann).await.map_err(heard)?;
        handle.register_session(2, ann).await.map_err(heard)?;
        let _version = handle
            .declare(
                0,
                0,
                vec![Declared::Predicate(DeclaredPredicate::new(PREDICATE))],
            )
            .await
            .map_err(heard)?;
        let (mut announced, mut evicted) = (false, false);
        for _ in 0..1_000 {
            let frame = compio::time::timeout(Duration::from_secs(5), storage.next_frame())
                .await
                .map_err(|_| TestError::new("no close came"))??;
            let view = frame.view()?;
            let close = EpochCloseView::from_frame(&view)?;
            announced |= close.announcement_count() > 0;
            evicted |= close.eviction(0) == Some(1u128.to_be_bytes());
            if announced && evicted {
                return Ok(());
            }
        }
        Err(TestError::new(format!(
            "announced {announced}, evicted {evicted}"
        )))
    })
}

/// A Members request after the owner stopped is refused as unavailable.
#[test]
fn test_members_after_the_owner_stopped_is_unavailable() -> TestResult {
    let data = directory("owner-stopped");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    std::fs::create_dir_all(&data)?;
    let clock = TestClock::new();
    let storage = SimStorage::new(13);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let cluster = cluster_under(&data)?;
        let address = role_endpoint(handle.clone(), &cluster, Readiness::default()).await?;
        // The next operation is the write of the next close's round, which stops the owner.
        storage.fail_at(storage.trace().len(), FaultMode::Before);
        clock.advance(EPOCH_US);
        assert!(ended(run).await?.is_err(), "the owner stopped");
        let password = cluster.secret().password();
        let mut compute = RoleClient::dial(&address, CLUSTER, "compute", password).await?;
        let refused = compute
            .request(&Members::new())
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(
            refusal_code(&refused),
            Some(ErrorCode::Unavailable),
            "{refused}"
        );
        drop(handle);
        Ok(())
    })
}
