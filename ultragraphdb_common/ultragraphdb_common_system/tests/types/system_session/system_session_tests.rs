/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_role::{
    ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server, serve_connection,
};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_common_system::{
    SystemHandle, SystemSession, SystemShared, SystemStart, start_system,
};
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportError, TransportErrorEnum};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyView, Cancel, Catalog, CloseRequest, Closed, Command, Declared, DeclaredPredicate,
    EpochCloseView, ErrorCode, ErrorReplyView, Join, Lease, Members, MembersView, Placement,
    Report, ReportView, RequestTerm, Subscribe,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{FaultMode, SimStorage};

use crate::types::system_group::{EPOCH_US, TestClock, ended, recovered, running, starting};

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
        let start = SystemStart::new(MetadataConfig::new(), sequencer, 1);
        let owner = start_system(
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
        let shared = shared_of(cluster, handle);
        let server =
            Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
        let credentials = secret.credentials()?;
        // Bound at port 0 and read back, so no other process takes the port between a choice and the bind.
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let address = listener.local_address()?;
        spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let (server, credentials) = (server.clone(), credentials.clone());
                let session = SystemSession::new(shared.clone());
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

/// What the role sessions over `owner` of `cluster` share.
fn shared_of(cluster: RoleCluster, owner: SystemHandle) -> SystemShared {
    SystemShared::new(cluster, owner)
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
    shared: SystemShared,
    cluster: &RoleCluster,
) -> TestResult {
    let server = Server::new(CLUSTER, "frontend", endpoint)?.with_iterations(ROLE_ITERATIONS);
    let credentials = cluster.secret().credentials()?;
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, credentials) = (server.clone(), credentials.clone());
            let session = SystemSession::new(shared.clone());
            spawn(async move { serve_connection(carrier, &server, credentials, session).await })
                .detach();
        }
    })
    .detach();
    Ok(())
}

/// A listener at a port of the loopback address the system chooses as it binds, so no other process takes the port between a
/// choice and the bind.
async fn loopback() -> TestResult<Listener> {
    Ok(Listener::bind(&Address::any_port("127.0.0.1")?).await?)
}

/// A role endpoint over `owner` of `cluster`: its address.
async fn role_endpoint(owner: SystemHandle, cluster: &RoleCluster) -> TestResult<Address> {
    let shared = shared_of(cluster.clone(), owner);
    let listener = loopback().await?;
    let address = listener.local_address()?;
    serve_roles(listener, Endpoint::Frontend, shared, cluster)?;
    Ok(address)
}

/// An owner over `data`, closing an epoch every 1 to 50 ms.
async fn owner_under(
    data: &std::path::Path,
) -> TestResult<ultragraphdb_common_system::SystemThread> {
    if data.exists() {
        std::fs::remove_dir_all(data)?;
    }
    std::fs::create_dir_all(data)?;
    let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
    Ok(start_system(
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
        let address = role_endpoint(handle, &cluster).await?;
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
        let address = role_endpoint(handle, &cluster).await?;
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

/// What a refused request was refused with.
fn refusal_code(refused: &TransportError) -> Option<ErrorCode> {
    if let TransportErrorEnum::Refused { code, .. } = &refused.0 {
        Some(*code)
    } else {
        None
    }
}

/// A session served where a partition's commands arrive serves none of them: a Closed request there is no command the system
/// group's endpoint table has any role send, and is refused as `CommandNotServed` naming it, closing the connection.
async fn refused_where_partitions_arrive(
    cluster: &RoleCluster,
    handle: SystemHandle,
) -> TestResult {
    let shared = shared_of(cluster.clone(), handle);
    let listener = loopback().await?;
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
        Some(ErrorCode::Protocol),
        "{refused}"
    );
    let rule =
        ultragraphdb_error_protocol::ProtocolError::command_not_served(Command::Closed.number());
    assert!(refused.to_string().contains(&rule.to_string()), "{refused}");
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
        let address = role_endpoint(handle.clone(), &cluster).await?;
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
        let address = role_endpoint(owner.handle().clone(), &cluster).await?;
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
        let start = SystemStart::new(
            MetadataConfig::new().with_sessions_max(1)?,
            SequencerConfig::new(1, 50)?,
            1,
        );
        let verifier = ScramVerifier::derive("pw", b"salt", 4_096)?;
        let owner = start_system(
            ServiceConfig::default(),
            CLUSTER,
            data.clone(),
            start,
            vec![("ann".to_string(), verifier)],
        )
        .await?;
        let handle = owner.handle().clone();
        let cluster = cluster_under(&data)?;
        let address = role_endpoint(handle.clone(), &cluster).await?;
        let password = cluster.secret().password();
        let mut storage = RoleClient::dial(&address, CLUSTER, "storage", password).await?;
        storage.send_only(&Subscribe::new(0)).await?;
        let heard = |refusal: ultragraphdb_protocol_wire::ErrorReply| {
            TestError::new(refusal.message().to_string())
        };
        let ann = handle.verifier("ann".to_string()).await.context("ann")?.0;
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
        let address = role_endpoint(handle.clone(), &cluster).await?;
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
