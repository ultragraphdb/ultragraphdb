/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios a shell decides: credit granted as batches are consumed, and TLS before the wire protocol. The test server
//! serves them, by the unit tests' server shared by path.

use std::cell::RefCell;
use std::rc::Rc;

use compio::net::TcpListener;
use compio::runtime::{Runtime, spawn};
use cucumber::{World, then, when};
use ultragraphdb_client_compio::{
    Anonymous, Client, ClientError, TcpConnector, TlsConnector, TlsDial, TlsError, TlsTrust,
    Transaction,
};
use ultragraphdb_client_core::{TEST_NAME, TEST_PASSWORD, TEST_PRINCIPAL, TestServer};
use ultragraphdb_protocol_scram::{MECHANISM, ScramLogin, system_nonce};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};
use ultragraphdb_protocol_transport_compio::CompioTlsStream;
use ultragraphdb_protocol_wire::{
    BodyView, Command, CreditView, FrameView, HeaderView, MAX_FRAME_SIZE, PatternSlot, QuadPattern,
    Read, TermTable, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};
use ultragraphdb_transport_tls::{TestPki, TlsErrorEnum};

/// The unit tests' server, shared by path.
pub mod utils {
    #[path = "serve.rs"]
    pub mod serve;
}

use crate::utils::serve::{CLUSTER, SimConnect, config, serve, serve_sim, welcome};

/// What each scenario leaves for its Then.
#[derive(Debug, Default, World)]
struct Steps {
    rows: u64,
    server: Option<Rc<RefCell<TestServer>>>,
    client: Option<Result<(), ClientError>>,
    principal: Option<u64>,
    written: bool,
    acceptor: Option<Result<(), TlsError>>,
}

fn runtime() -> TestResult<Runtime> {
    Runtime::new().context("a runtime")
}

/// A read of `tx`, naming it.
fn read(tx: &Transaction) -> Read {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Read::new(
        tx.stream(),
        0,
        terms,
        QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound(a),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        ),
        Vec::new(),
    )
}

/// A write of one quad.
fn insert() -> Write {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    Write::new(
        terms,
        vec![WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: a,
            object: a,
        }],
    )
}

#[when("a client reads 10,000 rows through a shell with an initial credit of 4 frames")]
fn long_read(w: &mut Steps) -> TestResult {
    // Two rows a frame: 5,000 frames.
    let frames = 5_000;
    let scripted = Rc::new(RefCell::new(
        TestServer::new(CLUSTER, welcome().with_initial_credit(4)).with_results(frames),
    ));
    let connector = SimConnect::new(SimNetwork::new(1, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    w.rows = runtime()?.block_on(async {
        let _server = spawn(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        let tx = client.begin(0, true).await?;
        let stream = client.read(&tx, &read(&tx)).await?;
        let mut rows = 0u64;
        while let Some(batch) = client.next_batch(&stream, |batch| batch.rows()).await? {
            rows += u64::from(batch);
        }
        client.close().await;
        Ok::<u64, TestError>(rows)
    })?;
    w.server = Some(scripted);
    Ok(())
}

#[then(
    "every row arrives, credit being granted as batches are consumed, and no grant of zero frames is sent"
)]
fn every_row(w: &mut Steps) -> TestResult {
    assert_eq!(w.rows, 10_000);
    let server = w.server.as_ref().context("a server")?.borrow();
    let credits: Vec<u32> = server
        .received()
        .iter()
        .filter(|f| {
            HeaderView::new(f).is_ok_and(|header| header.command() == Command::Credit.number())
        })
        .map(|f| {
            let view = FrameView::validate(f, CLUSTER, MAX_FRAME_SIZE)?;
            Ok(CreditView::from_frame(&view)?.frames())
        })
        .collect::<TestResult<_>>()?;
    assert!(!credits.is_empty(), "credit was granted");
    assert!(credits.iter().all(|frames| *frames > 0));
    Ok(())
}

/// A listener on a port of its own, and the connector to it.
async fn listener() -> TestResult<(TcpListener, TcpConnector)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let connector = TcpConnector::new(listener.local_addr()?);
    Ok((listener, connector))
}

/// One connection of `listener` accepted over TLS and served.
async fn accept_tls(
    listener: TcpListener,
    pki: TestPki,
    server: Rc<RefCell<TestServer>>,
) -> Result<(), TlsError> {
    let (stream, _peer) = listener
        .accept()
        .await
        .map_err(|e| TlsError::handshake_failed(String::new(), e.to_string()))?;
    let tls = CompioTlsStream::accept(&pki.acceptor()?, stream).await?;
    serve(&server, tls).await;
    Ok(())
}

/// A server asking for SCRAM-SHA-256.
fn scram_server() -> Rc<RefCell<TestServer>> {
    Rc::new(RefCell::new(TestServer::new(
        CLUSTER,
        welcome().with_mechanisms([MECHANISM]),
    )))
}

#[when(
    "the frontend holds the test CA's certificate for `localhost` and a client trusting the CA connects and authenticates"
)]
fn tls_client(w: &mut Steps) -> TestResult {
    let pki = TestPki::new()?;
    let trust = TlsTrust::from_pem("the test CA", pki.ca_pem().as_bytes())?;
    let scripted = scram_server();
    let (principal, written) = runtime()?.block_on(async {
        let (listener, tcp) = listener().await?;
        let _server = spawn(accept_tls(listener, pki, Rc::clone(&scripted)));
        let connector = TlsDial::new(tcp, TlsConnector::new(trust)?, "localhost");
        let login = ScramLogin::new(TEST_NAME, TEST_PASSWORD, system_nonce)?;
        let mut client = Client::new(connector, config(), login, 8);
        let session = client.register().await?;
        let principal = client.core().principal();
        let tx = client.begin(0, false).await?;
        client.write(&tx, &insert()).await?;
        let stream = client.read(&tx, &read(&tx)).await?;
        while client.next_batch(&stream, |_| ()).await?.is_some() {}
        let committed = client.commit(tx, session).await?;
        client.close().await;
        Ok::<_, TestError>((principal, committed.request() == 1))
    })?;
    w.principal = principal;
    w.written = written;
    w.server = Some(scripted);
    Ok(())
}

#[then("the TLS handshake, the wire handshake and SCRAM complete, and a write and a read succeed")]
fn tls_complete(w: &mut Steps) -> TestResult {
    assert_eq!(w.principal, Some(TEST_PRINCIPAL));
    assert!(w.written);
    let server = w.server.as_ref().context("a server")?.borrow();
    assert_eq!(server.applied(), 1);
    Ok(())
}

#[when(
    "a client without TLS sends the wire protocol's first frame to an endpoint configured for TLS"
)]
fn clear_text_client(w: &mut Steps) -> TestResult {
    let pki = TestPki::new()?;
    let scripted = Rc::new(RefCell::new(TestServer::new(CLUSTER, welcome())));
    let (client, acceptor) = runtime()?.block_on(async {
        let (listener, tcp) = listener().await?;
        let server = spawn(accept_tls(listener, pki, Rc::clone(&scripted)));
        let mut client = Client::new(tcp, config(), Anonymous, 8);
        let client = client.connect().await;
        let acceptor = server
            .await
            .map_err(|_panicked| TestError::new("the server task ended"))?;
        Ok::<_, TestError>((client, acceptor))
    })?;
    w.client = Some(client);
    w.acceptor = Some(acceptor);
    w.server = Some(scripted);
    Ok(())
}

#[then(
    "the connection fails as a TLS handshake failure, and no frame of the wire protocol is read"
)]
fn refused_as_tls(w: &mut Steps) -> TestResult {
    let acceptor = w.acceptor.take().context("the acceptor's end")?;
    let refusal = acceptor.err().context("a refusal")?;
    assert!(
        matches!(refusal.0, TlsErrorEnum::HandshakeFailed { .. }),
        "{refusal:?}"
    );
    assert!(
        w.client.take().context("the client's end")?.is_err(),
        "the client failed"
    );
    let server = w.server.as_ref().context("a server")?.borrow();
    assert!(server.received().is_empty(), "no frame was read");
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
