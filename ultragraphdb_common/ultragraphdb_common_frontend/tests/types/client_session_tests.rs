/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;
use std::time::{Duration, Instant};

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{
    AskedCredentials, AuthThrottle, ClientSession, EndpointCore, EndpointLimits,
    EveryAuthenticated, OwnerHandle, OwnerStart, RoleDouble, RoleLinks, TenantGate, last_frame,
    partition_answer, start_owner,
};
use ultragraphdb_common_role::{
    Answer, OpenRequests, RoleClient, RoleCluster, RoleSecret, Server, listen, serve_connection,
};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_protocol_scram::{Credentials, ScramVerifier};
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportErrorEnum};
use ultragraphdb_protocol_wire::{
    Begin, Command, Commit, ErrorCode, Join, Members, TermTable, Write, WriteChange,
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
    Ok(endpoint_within(owner, limits).await?.0)
}

/// A client endpoint over `owner` of `limits` at a free port; its address and its throttle.
async fn endpoint_within(
    owner: OwnerHandle,
    limits: EndpointLimits,
) -> TestResult<(Address, AuthThrottle)> {
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
    let throttle = core.throttle().clone();
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
    Ok((address, throttle))
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

#[expect(
    clippy::disallowed_methods,
    reason = "a point in time the throttle compares with, which no simulation replays"
)]
fn instant() -> Instant {
    Instant::now()
}

/// A principal past its failures is refused with its right password until its interval ends, as any failure is, and then serves.
#[test]
fn test_a_principal_past_its_failures_is_refused_until_its_interval_ends() -> TestResult {
    let data = directory("refused");
    if data.exists() {
        std::fs::remove_dir_all(&data)?;
    }
    let interval = Duration::from_millis(300);
    Runtime::new()?.block_on(async {
        let start = OwnerStart::new(MetadataConfig::new(), SequencerConfig::new(1, 50)?, 1);
        let principals = vec![(
            "alice".to_string(),
            ScramVerifier::derive("pw", b"alice's salt", ITERATIONS)?,
        )];
        let owner = start_owner(ServiceConfig::default(), CLUSTER, data, start, principals).await?;
        let limits =
            EndpointLimits::new(1 << 22, Duration::from_secs(30)).with_refusal_interval(interval);
        let (address, throttle) = endpoint_within(owner.handle().clone(), limits).await?;
        let failed_at = instant();
        for _ in 0..100 {
            throttle.name_failed("alice", failed_at);
        }
        let refused = RoleClient::dial(&address, CLUSTER, "alice", "pw")
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
        compio::time::sleep(interval).await;
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
