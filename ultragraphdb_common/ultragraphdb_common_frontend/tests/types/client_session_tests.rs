/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{
    AskedCredentials, ClientSession, EndpointCore, EndpointLimits, EveryAuthenticated, LoginLimits,
    OwnerHandle, OwnerStart, RoleDouble, RoleLinks, TenantGate, batch_frame, empty_batch,
    last_frame, partition_answer, start_owner,
};
use ultragraphdb_common_role::{
    Answer, OpenRequests, RoleClient, RoleCluster, RoleSecret, Server, listen, serve_connection,
};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_scram::{Credentials, ScramVerifier};
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportErrorEnum};
use ultragraphdb_protocol_wire::{
    Begin, BodyView, ColumnType, Command, Commit, DirectWrite, DropCommand, ErrorCode, Execute,
    Join, MAX_FRAME_FLOOR, MAX_FRAME_FLOOR_U32, Members, PatternSlot, Prepare, PrepareView,
    QuadPattern, Read, StatementCache, TermTable, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const CLUSTER: u64 = 7;
const ITERATIONS: u32 = 4_096;

fn directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("client-session-{name}-{}", std::process::id()))
}

/// A client endpoint over `owner` at a free port; its address.
async fn endpoint(owner: OwnerHandle) -> TestResult<Address> {
    let limits = EndpointLimits::new(1 << 22, Duration::from_secs(30));
    endpoint_within(owner, limits).await
}

/// A client endpoint over `owner` of `limits` at a free port, each connection admitted by the owner first as the frontend
/// admits it; its address.
async fn endpoint_within(owner: OwnerHandle, limits: EndpointLimits) -> TestResult<Address> {
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
    let core = EndpointCore::new(
        owner,
        RoleLinks::new(cluster, "frontend".to_string()),
        TenantGate::new(EveryAuthenticated),
        limits,
    );
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let credentials = AskedCredentials::default();
            let peer = carrier.peer_ip();
            let session = ClientSession::new(core.clone(), credentials.clone(), peer);
            let (server, core) = (server.clone(), core.clone());
            spawn(async move {
                if core.admits(peer).await {
                    serve_connection(carrier, &server, credentials, session).await;
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
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner = start_owner(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
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
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1)
            .with_logins(LoginLimits::new(1, 1, ban));
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner = start_owner(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let limits = EndpointLimits::new(1 << 22, Duration::from_secs(30));
        let address = endpoint_within(owner.handle().clone(), limits).await?;
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
        // The outcome is told to the owner without an answer: the ban holds once the owner has counted the second failure.
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
    owner: OwnerHandle,
    links: RoleLinks,
    open: OpenRequests,
) -> TestResult<Address> {
    let core = EndpointCore::new(
        owner,
        links,
        TenantGate::new(EveryAuthenticated),
        EndpointLimits::new(1 << 22, Duration::from_secs(30)),
    )
    .with_open(open.clone());
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
            let credentials = AskedCredentials::default();
            let session = ClientSession::new(core.clone(), credentials.clone(), None);
            let server = server.clone();
            spawn(async move { serve_connection(carrier, &server, credentials, session).await })
                .detach();
        }
    })
    .detach();
    Ok(address)
}

/// A client endpoint over `owner` reaching the roles through `links`, at a free port: its address.
async fn endpoint_over(owner: OwnerHandle, links: RoleLinks) -> TestResult<Address> {
    counted_endpoint(owner, links, OpenRequests::default()).await
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

/// A Commit the endpoint relayed is one of its open requests until its reply has left, so a frontend's stop waits for it
/// (role-binaries, *Readiness, health and shutdown*), and nothing stays counted once it has.
#[test]
fn test_a_relayed_commit_stays_open_until_its_reply_leaves() -> TestResult {
    let data = directory("relayed");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    Runtime::new()?.block_on(async {
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner = start_owner(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
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
        let links = RoleLinks::new(cluster, "frontend".to_string());
        links.set_map(owner.handle().map().await.context("a placement map")?);
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

/// A read the endpoint relayed is one of its open requests until its `LAST` has left, so a frontend's stop waits for it
/// (role-binaries, *Readiness, health and shutdown*), and nothing stays counted once it has.
#[test]
fn test_a_relayed_read_stays_open_until_its_last_leaves() -> TestResult {
    let data = directory("read-open");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    Runtime::new()?.block_on(async {
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner = start_owner(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let file = directory("read-open-secret");
        std::fs::write(&file, "a role secret")?;
        let cluster = RoleCluster::new(
            CLUSTER,
            RoleSecret::read(&file.display().to_string())?,
            Duration::from_secs(1),
        );
        let compute = RoleDouble::start(&cluster, Endpoint::Compute, |_| Answer::Open).await?;
        let links = RoleLinks::new(cluster, "frontend".to_string());
        links.set_map(owner.handle().map().await.context("a placement map")?);
        links.joined(&Join::compute(compute.address()));
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
        until("the read at compute", || {
            compute.count(Command::ComputeRead) == 1
        })
        .await?;
        assert_eq!(open.count(), 1, "the relayed read is open");
        let held = compute
            .frames(Command::ComputeRead)
            .first()
            .cloned()
            .context("the read held")?;
        held.answer_with(batch_frame(
            CLUSTER,
            held.stream(),
            &empty_batch(4, 1),
            true,
        )?);
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
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner = start_owner(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
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
        let links = RoleLinks::new(cluster, "frontend".to_string());
        links.set_map(owner.handle().map().await.context("a placement map")?);
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
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner = start_owner(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
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
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner = start_owner(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let file = directory("statements-secret");
        std::fs::write(&file, "a role secret")?;
        let cluster = RoleCluster::new(
            CLUSTER,
            RoleSecret::read(&file.display().to_string())?,
            Duration::from_secs(1),
        );
        let compute = RoleDouble::start(&cluster, Endpoint::Compute, |_| Answer::Open).await?;
        let links = RoleLinks::new(cluster, "frontend".to_string());
        links.set_map(owner.handle().map().await.context("a placement map")?);
        links.joined(&Join::compute(compute.address()));
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
        until("the first statement's read at compute", || {
            compute.count(Command::ComputeRead) == 1
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
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner = start_owner(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
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
