/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_compute::{
    AskedCredentials, AuthThrottle, ClientSession, ComputeCore, DoubleFrame, EndpointCore,
    EndpointLimits, EveryAuthenticated, LoginLimits, ReadTimes, RelayAuthenticator, RoleDouble,
    RoleLinks, SystemLink, TenantGate, last_frame, partition_answer, scan_answer,
};
use ultragraphdb_common_role::{
    Answer, OpenRequests, ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server, listen,
    serve_connection, serve_connection_with,
};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_common_system::{
    SystemHandle, SystemSession, SystemShared, SystemStart, SystemThread, start_system,
};
use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_scram::{Credentials, ScramVerifier};
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportErrorEnum};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    Begin, Bind, BindKind, BodyBuilder, BodyView, ColumnType, Command, Commit, CommitOperation,
    ConditionalWrite, DirectWrite, DropCommand, ErrorCode, ErrorReplyView, Execute, FLAG_LAST,
    Header, Held, Join, MAX_FRAME_FLOOR, MAX_FRAME_FLOOR_U32, Members, PatternSlot, Prepare,
    PrepareView, QuadPattern, Read, RequestTerm, StatementCache, TermTable, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const CLUSTER: u64 = 7;
const ITERATIONS: u32 = 4_096;

fn directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("client-session-{name}-{}", std::process::id()))
}

/// A cluster whose role secret is in a file of its own, apart from every other test's.
fn cluster_named(name: &str) -> TestResult<RoleCluster> {
    static FILES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let numbered = FILES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = directory(&format!("secret-{name}-{numbered}"));
    std::fs::write(&file, "a role secret")?;
    Ok(RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&file.display().to_string())?,
        Duration::from_secs(1),
    ))
}

/// `owner` served at a role address of its own, as the system group's host serves it, and a core's link to it reading
/// Members into `links`, once its view answers.
async fn linked(
    owner: SystemHandle,
    links: RoleLinks,
    name: &str,
) -> TestResult<(SystemLink, Address)> {
    let cluster = cluster_named(name)?;
    let roles = Server::new(CLUSTER, "compute", Endpoint::System)?.with_iterations(ROLE_ITERATIONS);
    let credentials = cluster.secret().credentials()?;
    let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
    let address = listener.local_address()?;
    let shared = SystemShared::new(cluster.clone(), owner);
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (roles, credentials) = (roles.clone(), credentials.clone());
            let session = SystemSession::new(shared.clone());
            spawn(async move { serve_connection(carrier, &roles, credentials, session).await })
                .detach();
        }
    })
    .detach();
    let link = SystemLink::dial(address.clone(), cluster, links, 5_000, Readiness::default());
    link.answering(
        compio::time::interval(Duration::from_secs(1)).tick().await + Duration::from_secs(5),
    )
    .await
    .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
    Ok((link, address))
}

/// A client endpoint over `owner` at a free port; its address.
async fn endpoint(owner: SystemHandle) -> TestResult<Address> {
    let limits = EndpointLimits::new(1 << 22, Duration::from_secs(30));
    endpoint_within(owner, LoginLimits::default(), limits).await
}

/// A client endpoint over `owner` of `limits` at a free port, each connection admitted by a throttle of `logins` first as
/// compute admits it; its address.
async fn endpoint_within(
    owner: SystemHandle,
    logins: LoginLimits,
    limits: EndpointLimits,
) -> TestResult<Address> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
    let address = listener.local_address()?;
    let server = Server::new(CLUSTER, "test", Endpoint::Client)?.with_iterations(ITERATIONS);
    let file = directory(&format!("secret-{port}"));
    std::fs::write(&file, "a role secret")?;
    let cluster = RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&file.display().to_string())?,
        Duration::from_secs(1),
    );
    let (throttle, throttling) = AuthThrottle::new(logins).serve();
    spawn(throttling).detach();
    let links = RoleLinks::new(cluster, "compute".to_string());
    let (link, _group) = linked(owner, links.clone(), &format!("link-{port}")).await?;
    let core = EndpointCore::new(
        link,
        throttle,
        links,
        TenantGate::new(EveryAuthenticated),
        limits,
    );
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let relay = RelayAuthenticator::default();
            let peer = carrier.peer_ip();
            let session = ClientSession::new(core.clone(), relay.clone(), peer);
            let (server, core) = (server.clone(), core.clone());
            spawn(async move {
                if core.admits(peer).await {
                    serve_connection_with(carrier, &server, relay, session).await;
                }
            })
            .detach();
        }
    })
    .detach();
    Ok(address)
}

/// A principal the owner holds authenticates on the client endpoint by the verifier it asks the owner for; a wrong
/// password and an unknown name are refused alike, and a request past authentication is not served yet.
#[test]
fn test_authentication_asks_the_owner_for_the_verifier() -> TestResult {
    let data = directory("verifier");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    Runtime::new()?.block_on(async {
        let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner =
            start_system(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let address = endpoint(owner.handle().clone()).await?;
        let mut alice = RoleClient::dial(&address, CLUSTER, "alice", "pw").await?;
        let asked = alice
            .request(&Members::new())
            .await
            .err()
            .context("a refusal")?;
        assert!(
            matches!(
                &asked.0,
                TransportErrorEnum::Refused {
                    code: ErrorCode::Protocol,
                    ..
                }
            ),
            "a role command is not served at the client endpoint: {asked}"
        );
        for (name, password) in [("alice", "wrong"), ("nobody", "pw")] {
            let refused = RoleClient::dial(&address, CLUSTER, name, password)
                .await
                .err()
                .context("a refusal")?;
            let code = if let TransportErrorEnum::Refused { code, .. } = &refused.0 {
                Some(*code)
            } else {
                None
            };
            assert_eq!(
                code,
                Some(ErrorCode::AuthenticationFailed),
                "{name}: {refused}"
            );
        }
        Ok(())
    })
}

/// What the owner answered reaches the machine for that name alone, once: nothing of it stays on the connection.
#[test]
fn test_an_answer_serves_its_own_name_once() -> TestResult {
    let credentials = AskedCredentials::default();
    let verifier = ScramVerifier::derive("pw", b"salt", ITERATIONS)?;
    credentials.answered("alice".to_string(), Some((7, verifier.clone())));
    assert!(credentials.lookup("bob").is_none());
    assert!(
        credentials.lookup("alice").is_none(),
        "the wrong name spent it"
    );
    credentials.answered("alice".to_string(), Some((7, verifier)));
    let (principal, _) = credentials.lookup("alice").context("the answer")?;
    assert_eq!(principal, 7);
    assert!(credentials.lookup("alice").is_none());
    credentials.answered("alice".to_string(), None);
    assert!(credentials.lookup("alice").is_none());
    Ok(())
}

/// A source past its failures is banned: its connection closes as accepted, before any frame, even with the right password, until
/// the ban ends on its own, and then it authenticates (design D15).
#[test]
fn test_a_source_past_its_failures_is_banned_until_its_ban_ends() -> TestResult {
    let data = directory("banned");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    let ban = Duration::from_millis(500);
    Runtime::new()?.block_on(async {
        let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner =
            start_system(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let limits = EndpointLimits::new(1 << 22, Duration::from_secs(30));
        let address =
            endpoint_within(owner.handle().clone(), LoginLimits::new(1, 1, ban), limits).await?;
        for _ in 0..2 {
            let refused = RoleClient::dial(&address, CLUSTER, "alice", "wrong")
                .await
                .err()
                .context("a refusal")?;
            assert!(
                matches!(
                    &refused.0,
                    TransportErrorEnum::Refused {
                        code: ErrorCode::AuthenticationFailed,
                        ..
                    }
                ),
                "{refused}"
            );
        }
        // The outcome is told to the throttle without an answer: the ban holds once it has counted the second failure.
        let mut closed = None;
        for _ in 0..100 {
            // A wrong password while the ban is not yet held counts once more, and a right one would forget the failures.
            match RoleClient::dial(&address, CLUSTER, "alice", "wrong").await {
                Err(error) if matches!(&error.0, TransportErrorEnum::Refused { .. }) => {
                    compio::time::sleep(Duration::from_millis(1)).await;
                }
                Err(error) => {
                    closed = Some(error);
                    break;
                }
                Ok(_accepted) => return Err(TestError::new("a wrong password authenticated")),
            }
        }
        let _closed = closed.context("a banned connection closed before any frame")?;
        compio::time::sleep(ban).await;
        let _alice = RoleClient::dial(&address, CLUSTER, "alice", "pw").await?;
        Ok(())
    })
}

/// The longest a step waits for a frame.
const WAIT: Duration = Duration::from_secs(5);

/// Waits until `holds`, each millisecond for at most 5 s, or says what it waited for.
async fn until(what: &str, holds: impl Fn() -> bool) -> TestResult {
    for _ in 0..5_000 {
        if holds() {
            return Ok(());
        }
        compio::time::sleep(Duration::from_millis(1)).await;
    }
    Err(TestError::new(format!("{what} did not come to hold")))
}

/// A client endpoint over `owner` reaching the roles through `links`, its connections and its sessions counted in `open`, at a
/// free port: its address.
async fn counted_endpoint(
    owner: SystemHandle,
    links: RoleLinks,
    open: OpenRequests,
) -> TestResult<Address> {
    let (throttle, throttling) = AuthThrottle::new(LoginLimits::default()).serve();
    spawn(throttling).detach();
    let map = links.map();
    let (link, group) = linked(owner, links.clone(), "counted").await?;
    if let Some(map) = map {
        links.set_map(map);
    }
    // The core's reads run in it, over its links to the partitions the group lists, beside its Bind streams (design D2).
    let compute = ComputeCore::over(
        links.clone(),
        group,
        ReadTimes::new(Duration::from_secs(30), Duration::from_secs(600)),
    );
    let core = EndpointCore::new(
        link,
        throttle,
        links,
        TenantGate::new(EveryAuthenticated),
        EndpointLimits::new(1 << 22, Duration::from_secs(30)),
    )
    .with_open(open.clone())
    .with_compute(compute);
    let server = Server::new(CLUSTER, "test", Endpoint::Client)?
        .with_iterations(ITERATIONS)
        .with_open(open);
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
    let address = listener.local_address()?;
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let relay = RelayAuthenticator::default();
            let session = ClientSession::new(core.clone(), relay.clone(), None);
            let server = server.clone();
            spawn(async move { serve_connection_with(carrier, &server, relay, session).await })
                .detach();
        }
    })
    .detach();
    Ok(address)
}

/// A client endpoint over `owner` reaching the roles through `links`, at a free port: its address.
async fn endpoint_over(owner: SystemHandle, links: RoleLinks) -> TestResult<Address> {
    counted_endpoint(owner, links, OpenRequests::default()).await
}

/// A partition answering as nothing goes wrong but each Scan, which it takes and answers none until a test answers it.
fn holding_scans(frame: &DoubleFrame) -> Answer {
    if frame.command() == Command::Scan {
        return Answer::Open;
    }
    partition_answer(frame, CLUSTER, 5 << 22)
}

/// A Write of one insert.
fn one_insert() -> Write {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.iri("http://ex.org/s"),
        predicate: terms.iri("http://ex.org/p"),
        object: terms.iri("http://ex.org/o"),
    };
    Write::new(terms, vec![change])
}

/// A Commit the endpoint relayed is one of its open requests until its reply has left, so compute's stop waits for it
/// (role-binaries, *Readiness, health and shutdown*), and nothing stays counted once it has.
#[test]
fn test_a_relayed_commit_stays_open_until_its_reply_leaves() -> TestResult {
    let data = directory("relayed");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    Runtime::new()?.block_on(async {
        let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner =
            start_system(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let file = directory("relayed-secret");
        std::fs::write(&file, "a role secret")?;
        let cluster = RoleCluster::new(
            CLUSTER,
            RoleSecret::read(&file.display().to_string())?,
            Duration::from_secs(1),
        );
        let partition = RoleDouble::start(&cluster, Endpoint::Partition, |frame| {
            if frame.command() == Command::Commit {
                return Answer::Open;
            }
            partition_answer(frame, CLUSTER, 5 << 22)
        })
        .await?;
        let links = RoleLinks::new(cluster, "compute".to_string());
        links.set_map(owner.handle().map().await.context("a placement map")?);
        // The system group lists it in Members, which the core's link reads into its links (design D9).
        owner
            .handle()
            .join(Join::storage(partition.address(), vec![1]))
            .await
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        links.joined(&Join::storage(partition.address(), vec![1]));
        let open = OpenRequests::default();
        let address = counted_endpoint(owner.handle().clone(), links, open.clone()).await?;
        let mut alice = RoleClient::dial(&address, CLUSTER, "alice", "pw").await?;
        // The reply holds the connection's one buffer until it is dropped.
        drop(
            compio::time::timeout(WAIT, alice.request(&Begin::new(0, 0, false)))
                .await
                .map_err(|_| TestError::new("the Begin was not answered"))??,
        );
        let transaction = alice.last_stream();
        alice.send_on(transaction, &one_insert()).await?;
        alice
            .send_on(transaction, &Commit::new([0; 16], 0, 0))
            .await?;
        until("the Commit at its partition", || {
            partition.count(Command::Commit) == 1
        })
        .await?;
        assert_eq!(open.count(), 1, "the relayed Commit is open");
        let held = partition
            .frames(Command::Commit)
            .first()
            .cloned()
            .context("the Commit held")?;
        held.answer_with(last_frame(
            CLUSTER,
            held.stream(),
            &Commit::new([0; 16], 0, 5 << 22),
        )?);
        let reply = compio::time::timeout(WAIT, alice.next_frame())
            .await
            .map_err(|_| TestError::new("the Commit's reply did not come"))??;
        assert_eq!(reply.view()?.command(), Command::Commit);
        until("nothing open", || open.count() == 0).await?;
        Ok(())
    })
}

/// A read the endpoint runs is one of its open requests until its `LAST` has left, so a stop waits for it
/// (role-binaries, *Readiness, health and shutdown*), and nothing stays counted once it has.
#[test]
fn test_a_read_stays_open_until_its_last_leaves() -> TestResult {
    let data = directory("read-open");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    Runtime::new()?.block_on(async {
        let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner =
            start_system(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let file = directory("read-open-secret");
        std::fs::write(&file, "a role secret")?;
        let cluster = RoleCluster::new(
            CLUSTER,
            RoleSecret::read(&file.display().to_string())?,
            Duration::from_secs(1),
        );
        let partition = RoleDouble::start(&cluster, Endpoint::Partition, holding_scans).await?;
        let links = RoleLinks::new(cluster, "compute".to_string());
        links.set_map(owner.handle().map().await.context("a placement map")?);
        // The system group lists it in Members, which the core reads (design D9).
        owner
            .handle()
            .join(Join::storage(partition.address(), vec![1]))
            .await
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        links.joined(&Join::storage(partition.address(), vec![1]));
        let open = OpenRequests::default();
        let address = counted_endpoint(owner.handle().clone(), links, open.clone()).await?;
        let mut alice = RoleClient::dial(&address, CLUSTER, "alice", "pw").await?;
        drop(
            compio::time::timeout(WAIT, alice.request(&Begin::new(0, 0, true)))
                .await
                .map_err(|_| TestError::new("the Begin was not answered"))??,
        );
        let transaction = alice.last_stream();
        let mut terms = TermTable::new();
        let pattern = QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound(terms.iri("http://ex.org/s")),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        );
        alice
            .send_only(&Read::new(transaction, 0, terms, pattern, Vec::new()))
            .await?;
        until("the read's Scan", || partition.count(Command::Scan) == 1).await?;
        assert_eq!(open.count(), 1, "the read is open");
        let held = partition
            .frames(Command::Scan)
            .first()
            .cloned()
            .context("the read's Scan held")?;
        match scan_answer(&held, CLUSTER, 5 << 22) {
            Answer::Reply(frame) => held.answer_with(frame),
            Answer::Open | Answer::Done | Answer::Refuse(_) | Answer::Close(_) => {
                return Err(TestError::new("the Scan has no answer"));
            }
        }
        let reply = compio::time::timeout(WAIT, alice.next_frame())
            .await
            .map_err(|_| TestError::new("the read's LAST did not come"))??;
        assert_eq!(reply.view()?.command(), Command::Result);
        until("nothing open", || open.count() == 0).await?;
        Ok(())
    })
}

/// A one-frame write and a Drop the endpoint relayed are each one of its open requests while their partition holds them.
#[test]
fn test_a_relayed_one_frame_write_and_drop_are_each_open() -> TestResult {
    let data = directory("writes-open");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    Runtime::new()?.block_on(async {
        let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner =
            start_system(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let file = directory("writes-open-secret");
        std::fs::write(&file, "a role secret")?;
        let cluster = RoleCluster::new(
            CLUSTER,
            RoleSecret::read(&file.display().to_string())?,
            Duration::from_secs(1),
        );
        let partition = RoleDouble::start(&cluster, Endpoint::Partition, |frame| {
            if matches!(frame.command(), Command::DirectWrite | Command::Drop) {
                return Answer::Open;
            }
            partition_answer(frame, CLUSTER, 5 << 22)
        })
        .await?;
        let links = RoleLinks::new(cluster, "compute".to_string());
        links.set_map(owner.handle().map().await.context("a placement map")?);
        // The system group lists it in Members, which the core's link reads into its links (design D9).
        owner
            .handle()
            .join(Join::storage(partition.address(), vec![1]))
            .await
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        links.joined(&Join::storage(partition.address(), vec![1]));
        let open = OpenRequests::default();
        let address = counted_endpoint(owner.handle().clone(), links, open.clone()).await?;
        let mut alice = RoleClient::dial(&address, CLUSTER, "alice", "pw").await?;
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri("http://ex.org/s"),
            predicate: terms.iri("http://ex.org/p"),
            object: terms.iri("http://ex.org/o"),
        };
        alice
            .send_only(&DirectWrite::new(0, terms, vec![change]))
            .await?;
        until("the one-frame write at its partition", || {
            partition.count(Command::DirectWrite) == 1
        })
        .await?;
        assert_eq!(open.count(), 1, "the one-frame write is open");
        alice.send_only(&DropCommand::tenant_data(0)).await?;
        until("the Drop at its partition", || {
            partition.count(Command::Drop) == 1
        })
        .await?;
        assert_eq!(open.count(), 2, "the Drop is open beside it");
        Ok(())
    })
}

/// A refusal naming a client's term longer than the client's `max_frame` reaches it within that frame, its message cut to fit,
/// so the client reads the refusal rather than closing the connection on a frame past its limit (`PROTOCOL.md` §9.4, §11.1).
#[test]
fn test_a_refusal_naming_a_term_past_the_client_s_frame_fits_it() -> TestResult {
    let data = directory("refusal-fits");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    Runtime::new()?.block_on(async {
        let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner =
            start_system(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let address = endpoint(owner.handle().clone()).await?;
        // 2^19, lossless.
        let mut alice =
            RoleClient::dial_taking(&address, CLUSTER, "alice", "pw", MAX_FRAME_FLOOR_U32).await?;
        let relative = "u".repeat(MAX_FRAME_FLOOR);
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri(&relative),
            predicate: terms.iri("http://ex.org/p"),
            object: terms.iri("http://ex.org/o"),
        };
        let refused = alice
            .request(&DirectWrite::new(0, terms, vec![change]))
            .await
            .err()
            .context("a refusal")?;
        let TransportErrorEnum::Refused { code, message } = &refused.0 else {
            return Err(TestError::new(format!("no refusal read: {refused}")));
        };
        assert_eq!(*code, ErrorCode::Refused);
        assert!(message.starts_with(relative.get(..64).context("the term's start")?));
        assert!(message.len() < relative.len(), "the term is cut");
        Ok(())
    })
}

/// A Prepare of the pattern binding the predicate `predicate`, taking no parameter.
fn prepared(predicate: &str) -> Prepare {
    let mut terms = TermTable::new();
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Bound(terms.iri(predicate)),
        PatternSlot::Unbound,
    );
    Prepare::new(
        0,
        0,
        0,
        terms,
        pattern,
        Vec::<ColumnType>::new(),
        Vec::new(),
    )
}

/// A connection holds the protocol's default count of statements: the first of that many still executes, and one more prepared
/// evicts it (`PROTOCOL.md` §11.9).
#[test]
fn test_a_connection_holds_the_default_count_of_statements() -> TestResult {
    let data = directory("statements");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    Runtime::new()?.block_on(async {
        let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner =
            start_system(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let file = directory("statements-secret");
        std::fs::write(&file, "a role secret")?;
        let cluster = RoleCluster::new(
            CLUSTER,
            RoleSecret::read(&file.display().to_string())?,
            Duration::from_secs(1),
        );
        let partition = RoleDouble::start(&cluster, Endpoint::Partition, holding_scans).await?;
        let links = RoleLinks::new(cluster, "compute".to_string());
        links.set_map(owner.handle().map().await.context("a placement map")?);
        // The system group lists it in Members, which the core reads (design D9).
        owner
            .handle()
            .join(Join::storage(partition.address(), vec![1]))
            .await
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        links.joined(&Join::storage(partition.address(), vec![1]));
        let address = endpoint_over(owner.handle().clone(), links).await?;
        let mut alice = RoleClient::dial(&address, CLUSTER, "alice", "pw").await?;
        let mut statements = Vec::with_capacity(StatementCache::DEFAULT_CAPACITY);
        for at in 0..StatementCache::DEFAULT_CAPACITY {
            let reply = alice
                .request(&prepared(&format!("http://ex.org/p{at}")))
                .await?;
            statements.push(PrepareView::from_frame(&reply.view()?)?.statement());
        }
        let first = *statements.first().context("a statement")?;
        drop(
            compio::time::timeout(WAIT, alice.request(&Begin::new(0, 0, true)))
                .await
                .map_err(|_| TestError::new("the Begin was not answered"))??,
        );
        let transaction = alice.last_stream();
        let execute = Execute::new(
            first,
            transaction,
            0,
            TermTable::new(),
            Vec::new(),
            Vec::new(),
        );
        alice.send_only(&execute).await?;
        until("the first statement's read", || {
            partition.count(Command::Scan) == 1
        })
        .await?;
        drop(alice.request(&prepared("http://ex.org/one-more")).await?);
        let evicted = alice.request(&execute).await.err().context("a refusal")?;
        assert!(
            matches!(
                &evicted.0,
                TransportErrorEnum::Refused {
                    code: ErrorCode::UnknownStatement,
                    ..
                }
            ),
            "the first statement evicted: {evicted}"
        );
        Ok(())
    })
}

/// A token of an epoch the sequencer has opened is within its open epoch: a Begin carrying one is served (`PROTOCOL.md` §11.5).
#[test]
fn test_a_token_of_an_opened_epoch_is_served() -> TestResult {
    let data = directory("token-open");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    Runtime::new()?.block_on(async {
        let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner =
            start_system(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let handle = owner.handle().clone();
        // Two closes past, the epoch the second opened is above both 0 and 1, and at most the open epoch however many closes
        // follow.
        let second = handle
            .next_close(1)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let opened = second.epoch().get() + 1;
        let address = endpoint(handle).await?;
        let mut alice = RoleClient::dial(&address, CLUSTER, "alice", "pw").await?;
        let token = SessionToken::new(1, 1, Epoch::new(opened)?);
        let begun = compio::time::timeout(
            WAIT,
            alice.request(&Begin::new(0, 0, false).with_token(token)),
        )
        .await
        .map_err(|_| TestError::new("the Begin was not answered"))??;
        assert_eq!(begun.view()?.command(), Command::Begin);
        Ok(())
    })
}

/// What the doubles answer next in place of their own answers: the command of the request answered and its answer, built on
/// the request's stream, each taken once, first in, first out.
type Armed = Rc<RefCell<Vec<(Command, fn(u64) -> Answer)>>>;

/// The answer `armed` holds for `frame`, taken, or `otherwise`'s.
fn armed_answer(
    armed: &Armed,
    frame: &DoubleFrame,
    otherwise: impl FnOnce(&DoubleFrame) -> Answer,
) -> Answer {
    let at = armed
        .borrow()
        .iter()
        .position(|(command, _)| *command == frame.command());
    match at {
        Some(at) => {
            let (_, answer) = armed.borrow_mut().remove(at);
            answer(frame.stream())
        }
        None => otherwise(frame),
    }
}

/// `frame` sent as the answer; nothing when it cannot be built.
fn replying(frame: Result<Vec<u8>, ProtocolError>) -> Answer {
    frame.map_or(Answer::Done, Answer::Reply)
}

/// `body` as a frame on `stream` flagged `flags`, byte 100 of its command header set: no command a role answers by uses it, so
/// each reads it as a byte its body's rules hold zero (`PROTOCOL.md` §6.3, §6.5, §13.10, §13.20).
fn broken<B: BodyBuilder>(body: &B, stream: u64, flags: u8) -> Result<Vec<u8>, ProtocolError> {
    let mut header = body.command_header();
    header[100] = 1;
    Header::new(CLUSTER, B::COMMAND)
        .with_stream(stream)
        .with_flags(flags)
        .with_command_header(header)
        .encode_frame(&body.body())
}

/// A Held on `stream`, whose body keeps its rules.
fn held(stream: u64) -> Answer {
    replying(Held.frame(Header::new(CLUSTER, Command::Held).with_stream(stream)))
}

/// An Error on `stream` whose body is three bytes no section table reads (`PROTOCOL.md` §9.4).
fn broken_error(stream: u64) -> Answer {
    replying(
        Header::new(CLUSTER, Command::Error)
            .with_stream(stream)
            .with_flags(FLAG_LAST)
            .encode_frame(&[0xFF; 3]),
    )
}

/// The endpoint's relays to a partition double, answering as `armed` says first, and otherwise as a partition does; `alice`
/// connected to the client endpoint, whose reads run over the core's own connection to the partition.
struct Relaying {
    alice: RoleClient,
    partition: RoleDouble,
    armed: Armed,
}

/// The relays over the system group `owner`, the role secret in a file of `name`.
async fn relaying(owner: &SystemThread, name: &str) -> TestResult<Relaying> {
    let file = directory(&format!("{name}-secret"));
    std::fs::write(&file, "a role secret")?;
    let cluster = RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&file.display().to_string())?,
        Duration::from_secs(1),
    );
    let armed = Armed::default();
    let partition = RoleDouble::start(&cluster, Endpoint::Partition, {
        let armed = Rc::clone(&armed);
        move |frame| {
            armed_answer(&armed, frame, |frame| {
                partition_answer(frame, CLUSTER, 5 << 22)
            })
        }
    })
    .await?;
    let links = RoleLinks::new(cluster, "compute".to_string());
    links.set_map(owner.handle().map().await.context("a placement map")?);
    // The system group lists it in Members, which the core's link reads into its links (design D9).
    for join in [Join::storage(partition.address(), vec![1])] {
        owner
            .handle()
            .join(join.clone())
            .await
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        links.joined(&join);
    }
    let address = endpoint_over(owner.handle().clone(), links).await?;
    let alice = RoleClient::dial(&address, CLUSTER, "alice", "pw").await?;
    Ok(Relaying {
        alice,
        partition,
        armed,
    })
}

/// A system group holding alice, its data in a directory of `name`.
async fn system_of(name: &str) -> TestResult<SystemThread> {
    let data = directory(name);
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
    let principals = vec![(
        "alice".to_string(),
        ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
    )];
    Ok(start_system(ServiceConfig::default(), CLUSTER, data, start, principals).await?)
}

impl Relaying {
    /// `answer` armed for the next request of `command`.
    fn arm(&self, command: Command, answer: fn(u64) -> Answer) {
        self.armed.borrow_mut().push((command, answer));
    }

    /// A read-write transaction begun: its stream.
    async fn begin(&mut self, read_only: bool) -> TestResult<u64> {
        // The reply holds the connection's one buffer until it is dropped.
        drop(
            compio::time::timeout(WAIT, self.alice.request(&Begin::new(0, 0, read_only)))
                .await
                .map_err(|_| TestError::new("the Begin was not answered"))??,
        );
        Ok(self.alice.last_stream())
    }

    /// The next frame alice reads: its stream and the refusal it holds.
    async fn refusal(&mut self) -> TestResult<(u64, ErrorCode)> {
        let frame = compio::time::timeout(WAIT, self.alice.next_frame())
            .await
            .map_err(|_| TestError::new("no answer came"))??;
        let view = frame.view()?;
        let stream = view.header().stream();
        let refusal = ErrorReplyView::from_frame(&view)
            .map_err(|rule| TestError::new(format!("{:?} on {stream}: {rule}", view.command())))?
            .to_body()?;
        Ok((stream, refusal.code()))
    }
}

/// A read of the transaction on `stream`, of what one subject holds.
fn subject_read(stream: u64) -> Read {
    let mut terms = TermTable::new();
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(terms.iri("http://ex.org/s")),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    Read::new(stream, 0, terms, pattern, Vec::new())
}

/// A partition answering a Bind with a frame compute refuses closes the role connection, as the side that dialled does
/// (`PROTOCOL.md` §2.6, §13): a body breaking its rules, a command a Bind is not answered by (§2.5, step 3), and an Error whose
/// body does not read. The transaction is answered 17, and the next is relayed over a connection dialled again, which a
/// partition's good answers commit.
#[test]
fn test_a_bind_answered_by_a_frame_refused_closes_the_link() -> TestResult {
    Runtime::new()?.block_on(async {
        let owner = system_of("bind-refused").await?;
        let mut relays = relaying(&owner, "bind-refused").await?;
        let answers: [fn(u64) -> Answer; 3] = [
            |stream| {
                let bind = Bind::new(1, BindKind::Transaction, 0, 0, 1, [1; 16])
                    .with_read_version(5 << 22);
                replying(broken(&bind, stream, 0))
            },
            held,
            broken_error,
        ];
        for (closed, answer) in (1..).zip(answers) {
            relays.arm(Command::Bind, answer);
            let transaction = relays.begin(false).await?;
            relays.alice.send_on(transaction, &one_insert()).await?;
            assert_eq!(
                relays.refusal().await?,
                (transaction, ErrorCode::Unavailable),
                "answer {closed}"
            );
            until("the role connection closed", || {
                relays.partition.connections_ended() == closed
            })
            .await?;
        }
        let transaction = relays.begin(false).await?;
        relays.alice.send_on(transaction, &one_insert()).await?;
        relays
            .alice
            .send_on(transaction, &Commit::new([0; 16], 0, 0))
            .await?;
        let reply = compio::time::timeout(WAIT, relays.alice.next_frame())
            .await
            .map_err(|_| TestError::new("the Commit's reply did not come"))??;
        assert_eq!(reply.view()?.command(), Command::Commit);
        Ok(())
    })
}

/// A Write's Held whose body breaks its rules closes the role connection, and the transaction is answered 17 (`PROTOCOL.md`
/// §13, §13.20).
#[test]
fn test_a_held_refused_closes_the_link() -> TestResult {
    Runtime::new()?.block_on(async {
        let owner = system_of("held-refused").await?;
        let mut relays = relaying(&owner, "held-refused").await?;
        relays.arm(Command::Write, |stream| replying(broken(&Held, stream, 0)));
        let transaction = relays.begin(false).await?;
        relays.alice.send_on(transaction, &one_insert()).await?;
        assert_eq!(
            relays.refusal().await?,
            (transaction, ErrorCode::Unavailable)
        );
        until("the role connection closed", || {
            relays.partition.connections_ended() == 1
        })
        .await?;
        Ok(())
    })
}

/// A Commit's reply whose body breaks its rules is not relayed: it closes the role connection, and the transaction is
/// answered 17 (`PROTOCOL.md` §6.3, §13).
#[test]
fn test_a_commit_reply_refused_closes_the_link() -> TestResult {
    Runtime::new()?.block_on(async {
        let owner = system_of("commit-refused").await?;
        let mut relays = relaying(&owner, "commit-refused").await?;
        relays.arm(Command::Commit, |stream| {
            replying(broken(&Commit::new([0; 16], 0, 5 << 22), stream, FLAG_LAST))
        });
        let transaction = relays.begin(false).await?;
        relays.alice.send_on(transaction, &one_insert()).await?;
        relays
            .alice
            .send_on(transaction, &Commit::new([0; 16], 0, 0))
            .await?;
        assert_eq!(
            relays.refusal().await?,
            (transaction, ErrorCode::Unavailable)
        );
        until("the role connection closed", || {
            relays.partition.connections_ended() == 1
        })
        .await?;
        Ok(())
    })
}

/// A Drop's reply whose body breaks its rules is not relayed: it closes the role connection, and the Drop is answered 17
/// (`PROTOCOL.md` §6.5, §13).
#[test]
fn test_a_drop_reply_refused_closes_the_link() -> TestResult {
    Runtime::new()?.block_on(async {
        let owner = system_of("drop-refused").await?;
        let mut relays = relaying(&owner, "drop-refused").await?;
        relays.arm(Command::Drop, |stream| {
            let reply = DropCommand::tenant_data(0).with_commit_version(5 << 22);
            replying(broken(&reply, stream, FLAG_LAST))
        });
        relays.alice.send_only(&DropCommand::tenant_data(0)).await?;
        let dropping = relays.alice.last_stream();
        assert_eq!(relays.refusal().await?, (dropping, ErrorCode::Unavailable));
        until("the role connection closed", || {
            relays.partition.connections_ended() == 1
        })
        .await?;
        Ok(())
    })
}

/// A claim's Reference answered with a body breaking its rules closes the role connection, and the write waiting on the
/// claim is answered 17 (`PROTOCOL.md` §13, §13.15).
#[test]
fn test_a_reference_refused_closes_the_link() -> TestResult {
    Runtime::new()?.block_on(async {
        let owner = system_of("reference-refused").await?;
        let mut relays = relaying(&owner, "reference-refused").await?;
        relays.arm(Command::Reference, |stream| {
            replying(
                Header::new(CLUSTER, Command::Reference)
                    .with_stream(stream)
                    .with_flags(FLAG_LAST)
                    .encode_frame(&[]),
            )
        });
        let mut terms = TermTable::new();
        let claim = CommitOperation::InsertIfAbsent {
            graph: None,
            predicate: terms.iri("http://ex.org/epi.id"),
            value: terms.intern(RequestTerm::Literal {
                lexical: "e-1".to_string(),
                datatype: None,
            })?,
            properties: Vec::new(),
        };
        relays
            .alice
            .send_only(&ConditionalWrite::new(0, terms, Vec::new(), vec![claim]))
            .await?;
        let writing = relays.alice.last_stream();
        assert_eq!(relays.refusal().await?, (writing, ErrorCode::Unavailable));
        until("the role connection closed", || {
            relays.partition.connections_ended() == 1
        })
        .await?;
        Ok(())
    })
}

/// A one-frame write answered by a command it is not answered by is not relayed: it closes the role connection, which
/// answers 17 every request waiting on it, a Drop relayed beside it too (`PROTOCOL.md` §2.5, step 3; §13).
#[test]
fn test_a_one_frame_write_answered_otherwise_closes_the_link() -> TestResult {
    Runtime::new()?.block_on(async {
        let owner = system_of("one-frame-refused").await?;
        let mut relays = relaying(&owner, "one-frame-refused").await?;
        relays.arm(Command::Drop, |_| Answer::Open);
        relays.alice.send_only(&DropCommand::tenant_data(0)).await?;
        let dropping = relays.alice.last_stream();
        until("the Drop at its partition", || {
            relays.partition.count(Command::Drop) == 1
        })
        .await?;
        relays.arm(Command::DirectWrite, held);
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri("http://ex.org/s"),
            predicate: terms.iri("http://ex.org/p"),
            object: terms.iri("http://ex.org/o"),
        };
        relays
            .alice
            .send_only(&DirectWrite::new(0, terms, vec![change]))
            .await?;
        let writing = relays.alice.last_stream();
        let mut answered = [relays.refusal().await?, relays.refusal().await?];
        answered.sort_unstable();
        assert_eq!(
            answered,
            [
                (dropping, ErrorCode::Unavailable),
                (writing, ErrorCode::Unavailable)
            ]
        );
        until("the role connection closed", || {
            relays.partition.connections_ended() == 1
        })
        .await?;
        Ok(())
    })
}

/// A partition answering a read's Scan with a command a Scan is not answered by, or with an Error whose body does not read,
/// closes the core's connection to it (`PROTOCOL.md` §2.5, step 3; §13): the read is answered 17.
#[test]
fn test_a_read_answered_by_a_frame_refused_closes_the_link() -> TestResult {
    Runtime::new()?.block_on(async {
        let owner = system_of("read-refused").await?;
        let mut relays = relaying(&owner, "read-refused").await?;
        let answers: [fn(u64) -> Answer; 2] = [held, broken_error];
        for (closed, answer) in (1..).zip(answers) {
            relays.arm(Command::Scan, answer);
            let transaction = relays.begin(true).await?;
            relays.alice.send_only(&subject_read(transaction)).await?;
            let reading = relays.alice.last_stream();
            assert_eq!(
                relays.refusal().await?,
                (reading, ErrorCode::Unavailable),
                "answer {closed}"
            );
            until("the read's connection closed", || {
                relays.partition.connections_ended() == closed
            })
            .await?;
        }
        Ok(())
    })
}
