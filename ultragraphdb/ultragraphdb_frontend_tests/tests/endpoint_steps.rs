/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend binary's client endpoint as a process: how it speaks, and who it knows at its first start, the other roles played by
//! this test.

use std::net::TcpStream;
use std::time::Duration;

use compio::io::{AsyncRead, AsyncWrite};
use compio::runtime::Runtime;
use cucumber::{then, when};
use service_utils::WaitStrategy;
use ultragraphdb_common_frontend::{RoleDouble, batch_frame, empty_batch, partition_answer};
use ultragraphdb_common_role::{Answer, RoleClient, RoleCluster, RoleSecret};
use ultragraphdb_protocol_scram::{ScramClient, system_nonce};
use ultragraphdb_protocol_transport::{
    Address, AuthenticationStep, BufferPool, ClientAuthentication, Endpoint, Framer, ReceivedFrame,
};
use ultragraphdb_protocol_transport_compio::{Carrier, CompioTlsStream, Connection};
use ultragraphdb_protocol_wire::{
    Begin, BodyBuilder, Command, DirectWrite, Header, Hello, Join, MAX_FRAME_SIZE, PatternSlot,
    QuadPattern, Read, TermTable, WriteChange,
};
use ultragraphdb_tests::{
    CLIENT_ADDRESS, CLUSTER, FRONTEND_BINARY, FRONTEND_OBSERVABILITY, FRONTEND_ROLE_ADDRESS,
    SECRET, START_SECONDS, answering, exit_is, frontend_holding, role, scratch, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::TestPki;

use crate::{Steps, services};

const ADMIN: &str = "admin";
const PASSWORD: &str = "the administrator's password";

/// The settings of a frontend bootstrapping its administrator.
pub(crate) fn bootstrapping() -> TestResult<Vec<(String, String)>> {
    let file = scratch("bootstrap-secret");
    std::fs::write(&file, PASSWORD)?;
    Ok(with(
        frontend_holding()?,
        "metadata.bootstrap_secret_file",
        &file.display().to_string(),
    ))
}

/// Those settings over TLS, the certificate `pki` issued.
fn over_tls(pki: &TestPki) -> TestResult<Vec<(String, String)>> {
    let (chain, key) = (scratch("server-chain.pem"), scratch("server-key.pem"));
    std::fs::write(&chain, pki.server_chain_pem())?;
    std::fs::write(&key, pki.server_key_pem())?;
    let mut settings = bootstrapping()?;
    settings.retain(|(name, _)| name != "endpoint.plaintext");
    let settings = with(
        settings,
        "endpoint.tls_cert_file",
        &chain.display().to_string(),
    );
    Ok(with(
        settings,
        "endpoint.tls_key_file",
        &key.display().to_string(),
    ))
}

fn started(w: &mut Steps, settings: &[(String, String)]) -> TestResult {
    let util = services()?;
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let _pid: u32 = util.start_service_from_config(role(
        FRONTEND_BINARY,
        settings,
        &scratch(FRONTEND_BINARY),
        wait,
    ))?;
    w.roles = Some(util);
    until_listening()
}

/// Waits for the client endpoint's listener, which opens after the first start has derived the administrator's verifier.
pub(crate) fn until_listening() -> TestResult {
    for _ in 0..START_SECONDS * 20 {
        if TcpStream::connect(CLIENT_ADDRESS).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(TestError::new("the client endpoint never listened"))
}

pub(crate) fn client_address() -> TestResult<Address> {
    Ok(Address::parse(&format!("tcp://{CLIENT_ADDRESS}"))?)
}

/// A client's connection after its handshake and authentication: what it writes next it numbers from `sent`.
pub(crate) struct Authenticated<S> {
    connection: Connection<S>,
    pool: BufferPool,
    sent: u128,
}

impl<S: AsyncRead + AsyncWrite> Authenticated<S> {
    pub(crate) async fn over(stream: S) -> TestResult<Self> {
        let mut held = Self {
            connection: Connection::new(stream, Framer::new(CLUSTER, MAX_FRAME_SIZE)),
            pool: BufferPool::new(1, MAX_FRAME_SIZE),
            sent: 1,
        };
        let hello = Hello::new(1, 1, 1, MAX_FRAME_SIZE as u32, "client");
        let _welcome = held.connection.hello(&held.pool, &hello).await?;
        let nonce = system_nonce()?;
        let mut exchange = ClientAuthentication::new(ScramClient::new(ADMIN, PASSWORD, &nonce)?);
        let first = exchange.start()?;
        held.send(first.frame(Header::new(CLUSTER, Command::Authenticate))?)
            .await?;
        loop {
            let answer = held.next().await?;
            let view = answer.view()?;
            match exchange.receive(&view)? {
                AuthenticationStep::Send(message) => {
                    let frame = message.frame(Header::new(CLUSTER, Command::Authenticate))?;
                    drop(answer);
                    held.send(frame).await?;
                }
                AuthenticationStep::Done { .. } => return Ok(held),
            }
        }
    }

    pub(crate) async fn send(&mut self, mut frame: Vec<u8>) -> TestResult {
        self.sent += 1;
        Header::stamp_sequence(&mut frame, self.sent)?;
        let _sent = self.connection.write_frame(frame).await?;
        Ok(())
    }

    pub(crate) async fn next(&mut self) -> TestResult<ReceivedFrame> {
        Ok(compio::time::timeout(
            Duration::from_secs(10),
            self.connection.read_frame(&self.pool),
        )
        .await
        .map_err(|_| TestError::new("no frame came"))??)
    }

    /// Sends `body` on `stream`, and reads the command of what answers it.
    pub(crate) async fn asked<B: BodyBuilder>(
        &mut self,
        stream: u64,
        body: &B,
    ) -> TestResult<Command> {
        self.send(body.frame(Header::new(CLUSTER, B::COMMAND).with_stream(stream))?)
            .await?;
        let answer = self.next().await?;
        Ok(answer.view()?.command())
    }
}

/// The cluster's role secret, as a test playing a role holds it.
fn played_cluster() -> TestResult<RoleCluster> {
    let file = scratch("played-secret");
    std::fs::write(&file, SECRET)?;
    Ok(RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&file.display().to_string())?,
        Duration::from_secs(5),
    ))
}

#[when(
    "the frontend holds the test CA's certificate for `localhost` and a client trusting the CA connects and authenticates"
)]
fn tls_client(w: &mut Steps) -> TestResult {
    let pki = TestPki::new()?;
    started(w, &over_tls(&pki)?)?;
    Runtime::new()?.block_on(async {
        let cluster = played_cluster()?;
        let frontend = Address::parse(&format!("tcp://{FRONTEND_ROLE_ADDRESS}"))?;
        let partition = RoleDouble::start(&cluster, Endpoint::Partition, |frame| {
            partition_answer(frame, CLUSTER, 5 << 22)
        })
        .await?;
        let join = Join::storage(partition.address(), vec![1]);
        partition
            .join(&frontend, &cluster, "storage", &join)
            .await
            .map_err(TestError::new)?;
        let compute = RoleDouble::start(&cluster, Endpoint::Compute, |frame| {
            batch_frame(CLUSTER, frame.stream(), &empty_batch(4, 1), true)
                .map_or(Answer::Done, Answer::Reply)
        })
        .await?;
        compute
            .join(
                &frontend,
                &cluster,
                "compute",
                &Join::compute(compute.address()),
            )
            .await
            .map_err(TestError::new)?;
        let carrier = Carrier::connect(&client_address()?).await?;
        let tls = CompioTlsStream::connect(&pki.connector()?, carrier, "localhost").await?;
        let mut client = Authenticated::over(tls).await?;
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri("http://ex.org/a"),
            predicate: terms.iri("http://ex.org/p"),
            object: terms.iri("http://ex.org/b"),
        };
        let write = DirectWrite::new(0, terms, vec![change]);
        assert_eq!(client.asked(1, &write).await?, Command::Commit, "a write");
        let begin = Begin::new(0, 0, true);
        assert_eq!(client.asked(2, &begin).await?, Command::Begin);
        let mut terms_2 = TermTable::new();
        let pattern = QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound(terms_2.iri("http://ex.org/a")),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        );
        let read = Read::new(2, 0, terms_2, pattern, Vec::new());
        assert_eq!(client.asked(3, &read).await?, Command::Result, "a read");
        Ok(())
    })
}

#[then("the TLS handshake, the wire handshake and SCRAM complete, and a write and a read succeed")]
fn tls_served(w: &mut Steps) {
    w.roles = None;
}

#[when(
    "a client without TLS sends the wire protocol's first frame to an endpoint configured for TLS"
)]
fn clear_text_at_tls(w: &mut Steps) -> TestResult {
    let pki = TestPki::new()?;
    started(w, &over_tls(&pki)?)?;
    Runtime::new()?.block_on(async {
        let carrier = Carrier::connect(&client_address()?).await?;
        let mut raw = Connection::new(carrier, Framer::new(CLUSTER, MAX_FRAME_SIZE));
        let pool = BufferPool::new(1, MAX_FRAME_SIZE);
        let hello = Hello::new(1, 1, 1, MAX_FRAME_SIZE as u32, "client");
        let refused = raw.hello(&pool, &hello).await.err().context("no Welcome")?;
        w.refused = Some(refused.to_string());
        Ok(())
    })
}

#[then(
    "the connection fails as a TLS handshake failure, and no frame of the wire protocol is read"
)]
fn clear_text_refused(w: &mut Steps) -> TestResult {
    let refused = w.refused.take().context("a failure")?;
    assert!(!refused.is_empty());
    w.roles = None;
    Ok(())
}

#[when(
    "the frontend starts with neither a certificate nor `plaintext = true` in its `[endpoint]` table"
)]
fn no_decision(w: &mut Steps) -> TestResult {
    let mut settings = frontend_holding()?;
    settings.retain(|(name, _)| name != "endpoint.plaintext");
    let log = scratch("frontend-no-decision.log");
    let util = services()?;
    let pid = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &settings,
        &log,
        WaitStrategy::NoWait,
    ))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(20))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 78 and an error naming `tls_cert_file`, `tls_key_file` and `plaintext`")]
fn no_decision_named(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    for setting in ["tls_cert_file", "tls_key_file", "plaintext"] {
        assert!(w.log.contains(setting), "{setting}: {}", w.log);
    }
    Ok(())
}

#[when(
    "a cluster starts for the first time with a bootstrap credential file and a client authenticates with it"
)]
fn bootstrapped(w: &mut Steps) -> TestResult {
    started(w, &bootstrapping()?)?;
    Runtime::new()?.block_on(async {
        let _client = RoleClient::dial(&client_address()?, CLUSTER, ADMIN, PASSWORD).await?;
        Ok(())
    })
}

#[then("authentication succeeds, and the metadata group holds that principal without the password")]
fn bootstrap_held(w: &mut Steps) -> TestResult {
    let wrong = Runtime::new()?.block_on(async {
        RoleClient::dial(&client_address()?, CLUSTER, ADMIN, "not it")
            .await
            .err()
            .map(|error| error.to_string())
            .context("a wrong password is refused")
    })?;
    assert!(wrong.contains("authentication failed"), "{wrong}");
    let log = std::fs::read_to_string(scratch(FRONTEND_BINARY))?;
    for outcome in ["accepted", "refused"] {
        let audited = log
            .lines()
            .any(|line| line.contains("audit:") && line.contains(outcome) && line.contains(ADMIN));
        assert!(audited, "the audit of {outcome}: {log}");
    }
    assert!(!log.contains(PASSWORD), "a credential in the log");
    w.roles = None;
    Ok(())
}
