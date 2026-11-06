/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::task::LocalSet;

use ultragraphdb_client_tokio::{Anonymous, Client, ClientError, Connect, TokioSimStream};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork, TransportError};

use crate::utils::serve::{SimConnect, config, in_doubt, serve_sim, server};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

fn runtime() -> TestResult<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("a runtime")
}

#[test]
fn test_close_gives_the_connection_up() -> TestResult {
    let rt = runtime()?;
    let scripted = server();
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    LocalSet::new().block_on(&rt, async {
        let _server: tokio::task::JoinHandle<()> =
            tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        client.connect().await?;
        assert!(client.core().is_ready());
        client.close().await;
        assert!(!client.core().is_ready());
        assert!(!client.core().is_connected());
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A simulated stream whose first read fails with `fault`, and every write with `write_fault` when set, the rest served
/// by the stream.
struct Faulty {
    stream: TokioSimStream,
    fault: Option<io::ErrorKind>,
    write_fault: Option<io::ErrorKind>,
}

impl AsyncRead for Faulty {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if let Some(kind) = self.fault.take() {
            return Poll::Ready(Err(kind.into()));
        }
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for Faulty {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if let Some(kind) = self.write_fault {
            return Poll::Ready(Err(kind.into()));
        }
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

/// Connections over the simulated network whose first connection's first read fails with `fault`, and whose writes
/// all fail with `write_fault`.
struct FaultyConnect {
    inner: SimConnect,
    fault: Option<io::ErrorKind>,
    write_fault: Option<io::ErrorKind>,
}

impl Connect for FaultyConnect {
    type Stream = Faulty;

    async fn connect(&mut self) -> io::Result<Faulty> {
        let stream = self.inner.connect().await?;
        Ok(Faulty {
            stream,
            fault: self.fault.take(),
            write_fault: self.write_fault,
        })
    }
}

/// `client.connect()` over a connection whose first read fails with `fault`.
fn connect_with_fault(fault: io::ErrorKind) -> TestResult<Result<(), ClientError>> {
    connect_with_faults(Some(fault), None)
}

/// `client.connect()` over a connection whose first read fails with `fault` and whose writes fail with `write_fault`.
fn connect_with_faults(
    fault: Option<io::ErrorKind>,
    write_fault: Option<io::ErrorKind>,
) -> TestResult<Result<(), ClientError>> {
    let rt = runtime()?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = FaultyConnect {
        inner,
        fault,
        write_fault,
    };
    Ok(LocalSet::new().block_on(&rt, async {
        let _server: tokio::task::JoinHandle<()> =
            tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        let connected = client.connect().await;
        assert_eq!(connected.is_ok(), client.core().is_ready());
        connected
    }))
}

#[test]
fn test_an_interrupted_read_is_retried() -> TestResult {
    // A signal interrupting the read is no failure: the read is made again and the handshake completes.
    assert_eq!(connect_with_fault(io::ErrorKind::Interrupted)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_failed_read_fails_the_connection_with_its_error() -> TestResult {
    // A reset connection is the stream's failure, as `TransportError::io` records it: its kind and its text.
    assert_eq!(
        connect_with_fault(io::ErrorKind::ConnectionReset)?,
        Err(ClientError::transport(TransportError::io(
            &io::ErrorKind::ConnectionReset.into()
        )))
    );
    Ok(())
}

#[test]
fn test_a_failed_write_fails_the_connection_with_its_error() -> TestResult {
    // The Hello cannot be written: the connection fails with the write's error, as a failed read's.
    assert_eq!(
        connect_with_faults(None, Some(io::ErrorKind::BrokenPipe))?,
        Err(ClientError::transport(TransportError::io(
            &io::ErrorKind::BrokenPipe.into()
        )))
    );
    Ok(())
}

/// Connections refused with `refusal`, or, with none, never made.
struct Unreachable {
    refusal: Option<io::ErrorKind>,
}

impl Connect for Unreachable {
    type Stream = TokioSimStream;

    async fn connect(&mut self) -> io::Result<TokioSimStream> {
        match self.refusal {
            Some(kind) => Err(kind.into()),
            None => std::future::pending().await,
        }
    }
}

#[test]
fn test_a_refused_connection_is_the_transport_error() -> TestResult {
    let rt = runtime()?;
    let refused = LocalSet::new().block_on(&rt, async {
        let connector = Unreachable {
            refusal: Some(io::ErrorKind::ConnectionRefused),
        };
        let mut client = Client::new(connector, config(), Anonymous, 2);
        let refused = client.register().await;
        assert!(!client.core().is_connected());
        refused
    });
    assert_eq!(
        refused,
        Err(ClientError::transport(TransportError::io(
            &io::ErrorKind::ConnectionRefused.into()
        )))
    );
    Ok(())
}

#[test]
fn test_a_connection_not_made_within_the_timeout_times_out() -> TestResult {
    let rt = runtime()?;
    let timed_out = LocalSet::new().block_on(&rt, async {
        let connector = Unreachable { refusal: None };
        let config = config().with_timeout(Duration::from_millis(20));
        let mut client = Client::new(connector, config, Anonymous, 2);
        client.connect().await
    });
    assert_eq!(timed_out, Err(ClientError::timed_out()));
    Ok(())
}

/// Connections over the simulated network: the first made, then `failing` refused, each refusal using one up, then
/// made again; `made` counts every attempt. Both are shared, so a test reads and changes them while the client holds
/// the connector.
struct Flaky {
    inner: SimConnect,
    made: Rc<std::cell::Cell<u32>>,
    failing: Rc<std::cell::Cell<u32>>,
}

impl Flaky {
    fn new(inner: SimConnect, failing: u32) -> Self {
        Self {
            inner,
            made: Rc::new(std::cell::Cell::new(0)),
            failing: Rc::new(std::cell::Cell::new(failing)),
        }
    }
}

impl Connect for Flaky {
    type Stream = TokioSimStream;

    async fn connect(&mut self) -> io::Result<TokioSimStream> {
        self.made.set(self.made.get() + 1);
        if self.made.get() > 1 && self.failing.get() > 0 {
            self.failing.set(self.failing.get() - 1);
            return Err(io::ErrorKind::ConnectionRefused.into());
        }
        self.inner.connect().await
    }
}

/// A commit whose reply was lost is retried on the next connection the shell manages to make, refused connections
/// before it notwithstanding, and applied once.
#[test]
fn test_a_lost_commit_is_retried_once_a_connection_is_made_again() -> TestResult {
    let rt = runtime()?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = Flaky::new(inner, 3);
    LocalSet::new().block_on(&rt, async {
        let _server = tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        let session = client.register().await?;
        let tx = client.begin(0, false).await?;
        scripted.borrow_mut().drop_after_next_commit();
        let committed = client.commit(tx, session).await?;
        assert_eq!(committed.request(), 1);
        assert_eq!(scripted.borrow().applied(), 1, "applied once");
        assert_eq!(client.core().next_request(session), Some(2));
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A server unreachable while a commit waits to be retried ends the retries: each refused connection counts as one
/// of the commit's attempts, so the connector is asked exactly `commit_attempts` times in all, the first connection
/// included, and the commit is in doubt. Once the server is reachable again, resolving it settles it from the
/// server's record: applied once.
#[test]
fn test_refused_reconnects_are_bounded_by_the_commit_attempts() -> TestResult {
    let rt = runtime()?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = Flaky::new(inner, 5_000);
    let (made, failing) = (Rc::clone(&connector.made), Rc::clone(&connector.failing));
    LocalSet::new().block_on(&rt, async {
        let _server = tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config().with_commit_attempts(4), Anonymous, 8);
        let session = client.register().await?;
        let tx = client.begin(0, false).await?;
        scripted.borrow_mut().drop_after_next_commit();
        let exhausted = client.commit(tx, session).await.err().context("an error")?;
        assert_eq!(in_doubt(&exhausted), Some((session.number(), 1)));
        // The first connection, then three refused: the send lost with its connection and three failed attempts.
        assert_eq!(made.get(), 4);
        assert_eq!(scripted.borrow().applied(), 1, "the first send was applied");
        failing.set(0);
        let resolved = client.resolve(session).await?;
        assert_eq!(resolved.request(), 1);
        assert_eq!(scripted.borrow().applied(), 1, "applied once");
        assert_eq!(client.core().next_request(session), Some(2));
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// Reconnecting for a commit waits the retry delay on the runtime's timer between attempts, so an outer timeout
/// around the commit cancels it while the server stays unreachable.
#[test]
fn test_an_outer_timeout_cancels_a_commit_whose_server_is_unreachable() -> TestResult {
    let rt = runtime()?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = Flaky::new(inner, u32::MAX);
    let made = Rc::clone(&connector.made);
    LocalSet::new().block_on(&rt, async {
        let _server = tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let config = config()
            .with_commit_attempts(u32::MAX)
            .with_retry_delay(Duration::from_millis(5));
        let mut client = Client::new(connector, config, Anonymous, 8);
        let session = client.register().await?;
        let tx = client.begin(0, false).await?;
        scripted.borrow_mut().drop_after_next_commit();
        let cancelled =
            tokio::time::timeout(Duration::from_millis(100), client.commit(tx, session)).await;
        assert!(
            cancelled.is_err(),
            "the commit is still retrying when the outer timeout ends it"
        );
        // At 5 ms apart, about 20 attempts fit in 100 ms; without the delay there would be thousands, or none at all
        // if the loop never yielded.
        let attempts = made.get();
        assert!(
            (2..=40).contains(&attempts),
            "{attempts} connections asked for"
        );
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// Connections over the simulated network: the first served, every later one hung up by the server before its
/// handshake ends; `made` counts every connection asked for.
struct Hangup {
    inner: SimConnect,
    made: Rc<std::cell::Cell<u32>>,
}

impl Connect for Hangup {
    type Stream = TokioSimStream;

    async fn connect(&mut self) -> io::Result<TokioSimStream> {
        self.made.set(self.made.get() + 1);
        if self.made.get() == 1 {
            return self.inner.connect().await;
        }
        let (client, server) = self.inner.network.connect();
        server.shutdown();
        Ok(TokioSimStream::new(client))
    }
}

/// A server that accepts connections and hangs up every handshake: the shell waits the retry delay before each
/// reconnect, so the retries of a commit with attempts to spare yield and an outer timeout ends them.
#[test]
fn test_reconnects_after_handshakes_hung_up_wait_the_retry_delay() -> TestResult {
    let rt = runtime()?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = Hangup {
        inner,
        made: Rc::new(std::cell::Cell::new(0)),
    };
    let made = Rc::clone(&connector.made);
    LocalSet::new().block_on(&rt, async {
        let _server = tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let config = config()
            .with_commit_attempts(u32::MAX)
            .with_retry_delay(Duration::from_millis(5));
        let mut client = Client::new(connector, config, Anonymous, 8);
        let session = client.register().await?;
        let tx = client.begin(0, false).await?;
        scripted.borrow_mut().drop_after_next_commit();
        let cancelled =
            tokio::time::timeout(Duration::from_millis(100), client.commit(tx, session)).await;
        assert!(
            cancelled.is_err(),
            "the commit is still retrying when the outer timeout ends it"
        );
        // At 5 ms apart, about 20 connections fit in 100 ms; without the delay the loop never yields.
        let connections = made.get();
        assert!(
            (3..=40).contains(&connections),
            "{connections} connections asked for"
        );
        assert_eq!(scripted.borrow().applied(), 1, "the first send was applied");
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// Each handshake hung up while a commit waits to be sent again uses one of its attempts, as a connection not made does,
/// so the retries end with the commit in doubt: the first send and three lost handshakes are its four attempts.
#[test]
fn test_handshakes_hung_up_end_a_commits_retries_within_its_attempts() -> TestResult {
    let rt = runtime()?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = Hangup {
        inner,
        made: Rc::new(std::cell::Cell::new(0)),
    };
    let made = Rc::clone(&connector.made);
    LocalSet::new().block_on(&rt, async {
        let _server = tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let config = config()
            .with_commit_attempts(4)
            .with_retry_delay(Duration::from_millis(5));
        let mut client = Client::new(connector, config, Anonymous, 8);
        let session = client.register().await?;
        let tx = client.begin(0, false).await?;
        scripted.borrow_mut().drop_after_next_commit();
        let exhausted = tokio::time::timeout(Duration::from_secs(2), client.commit(tx, session))
            .await
            .context("the retries end within their attempts")?
            .err()
            .context("in doubt")?;
        assert_eq!(in_doubt(&exhausted), Some((session.number(), 1)));
        // The first connection, then three hung up: the send lost with its connection and three lost handshakes.
        assert_eq!(made.get(), 4);
        assert_eq!(scripted.borrow().applied(), 1, "the first send was applied");
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A one-frame write of one statement.
fn insert() -> ultragraphdb_protocol_wire::DirectWrite {
    let mut terms = ultragraphdb_protocol_wire::TermTable::new();
    let a = terms.iri("http://ex.org/a");
    ultragraphdb_protocol_wire::DirectWrite::new(
        0,
        terms,
        vec![ultragraphdb_protocol_wire::WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: a,
            object: a,
        }],
    )
}

/// A read of `tx`, naming it.
fn read(tx: &ultragraphdb_client_tokio::Transaction) -> ultragraphdb_protocol_wire::Read {
    use ultragraphdb_protocol_wire::{PatternSlot, QuadPattern, Read, TermTable};
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

/// The batches of `stream`, read to its end.
async fn batches<C: Connect, L: ultragraphdb_client_tokio::Login>(
    client: &mut Client<C, L>,
    stream: &ultragraphdb_client_tokio::ResultStream,
) -> TestResult<usize> {
    let mut count = 0;
    while client.next_batch(stream, |_| ()).await?.is_some() {
        count += 1;
    }
    Ok(count)
}

/// A deadline that is no timeout, a busy write's resend, leaves the connection be: the resend goes on the same connection
/// while a read is out, and a read open beside it reads on to its end.
#[test]
fn test_a_busy_resend_keeps_the_connection_and_its_reads() -> TestResult {
    let rt = runtime()?;
    let results = 3;
    let scripted = Rc::new(std::cell::RefCell::new(server_of(1).with_results(results)));
    scripted
        .borrow_mut()
        .refuse_next_commit(ultragraphdb_protocol_wire::ErrorCode::SessionBusy, 0);
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    LocalSet::new().block_on(&rt, async {
        let _server = tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        let session = client.register().await?;
        let reader = client.begin(0, true).await?;
        let stream = client.read(&reader, &read(&reader)).await?;
        let written = client.direct_write(insert(), session).await?;
        assert_eq!(written.request(), 1);
        assert_eq!(scripted.borrow().applied(), 1, "the resend applied it");
        assert_eq!(batches(&mut client, &stream).await?, results as usize);
        assert!(client.core().is_ready());
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// The test server, its Welcome granting `credit` frames to a read.
fn server_of(credit: u32) -> ultragraphdb_client_core::TestServer {
    ultragraphdb_client_core::TestServer::new(
        crate::utils::serve::CLUSTER,
        crate::utils::serve::welcome().with_initial_credit(credit),
    )
}

/// A read not yet polled never costs the connection: the client opens a read only while its buffers hold every frame the
/// read's credit and a refusal can bring, so a second read past them is refused before it is sent, and a request answered
/// while the first read's frames wait finds a buffer free; the first read then reads on to its end. Here a read reserves
/// the Welcome's 4 frames and a refusal, and 6 buffers keep one to read into.
#[test]
fn test_a_read_not_yet_polled_never_costs_the_connection() -> TestResult {
    let rt = runtime()?;
    let results = 6;
    let scripted = Rc::new(std::cell::RefCell::new(server_of(4).with_results(results)));
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    LocalSet::new().block_on(&rt, async {
        let _server = tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let buffers = 6;
        let mut client = Client::new(connector, config(), Anonymous, buffers);
        let reader = client.begin(0, true).await?;
        let first = client.read(&reader, &read(&reader)).await?;
        let second = client.read(&reader, &read(&reader)).await;
        assert_eq!(second.err(), Some(ClientError::buffer_limit(buffers)));
        let session = client.register().await?;
        let written = client.direct_write(insert(), session).await?;
        assert_eq!(written.request(), 1);
        assert_eq!(batches(&mut client, &first).await?, results as usize);
        assert!(client.core().is_ready());
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A credential's renewal, a deadline that is no timeout, goes out on the open connection while a read is out, and the
/// read goes on: its batches all arrive on the connection the renewal kept.
#[test]
fn test_a_renewal_keeps_the_connection_and_its_reads() -> TestResult {
    use ultragraphdb_client_core::{TEST_NAME, TEST_PASSWORD, TEST_PRINCIPAL};
    use ultragraphdb_protocol_scram::{MECHANISM, ScramLogin, system_nonce};
    let rt = runtime()?;
    let results = 3;
    // Renewed at half its life.
    let life = Duration::from_millis(400);
    let scripted = Rc::new(std::cell::RefCell::new(
        ultragraphdb_client_core::TestServer::new(
            crate::utils::serve::CLUSTER,
            crate::utils::serve::welcome().with_mechanisms([MECHANISM]),
        )
        .with_credential_life(life)
        .with_results(results),
    ));
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    LocalSet::new().block_on(&rt, async {
        let _server = tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let login = ScramLogin::new(TEST_NAME, TEST_PASSWORD, system_nonce)?;
        let mut client = Client::new(connector, config(), login, 8);
        client.connect().await?;
        let first = client.core().expires().context("an expiry")?;
        let reader = client.begin(0, true).await?;
        // The renewal falls due before the read is sent, so it goes out while the read's first frame is awaited.
        tokio::time::sleep(life / 2).await;
        let stream = client.read(&reader, &read(&reader)).await?;
        assert_eq!(batches(&mut client, &stream).await?, results as usize);
        // The exchange's last frame comes before the answer to a request sent after it.
        let _session = client.register().await?;
        let renewed = client.core().expires().context("an expiry")?;
        assert!(renewed > first, "the credential was renewed");
        assert_eq!(client.core().principal(), Some(TEST_PRINCIPAL));
        assert!(client.core().is_ready());
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A stream over the simulated network whose writes stop being taken once `stalled` is set, as a peer that stopped
/// reading leaves them.
#[derive(Debug)]
struct Stalling {
    inner: TokioSimStream,
    stalled: Rc<std::cell::Cell<bool>>,
}

impl AsyncRead for Stalling {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Stalling {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.stalled.get() {
            return Poll::Pending;
        }
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.stalled.get() {
            return Poll::Pending;
        }
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Connections over the simulated network whose writes stall once `stalled` is set.
struct StallConnect {
    sim: SimConnect,
    stalled: Rc<std::cell::Cell<bool>>,
}

impl Connect for StallConnect {
    type Stream = Stalling;

    async fn connect(&mut self) -> io::Result<Stalling> {
        Ok(Stalling {
            inner: self.sim.connect().await?,
            stalled: Rc::clone(&self.stalled),
        })
    }
}

/// A peer that stops taking frames bounds a request by the timeout: the write blocked on it times out, the connection is
/// given up, and the request fails, its shutdown bounded the same way.
#[test]
fn test_a_write_the_peer_never_takes_times_out() -> TestResult {
    let rt = runtime()?;
    let scripted = server();
    let sim = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&sim.accepted);
    let stalled = Rc::new(std::cell::Cell::new(false));
    let connector = StallConnect {
        sim,
        stalled: Rc::clone(&stalled),
    };
    let timeout = Duration::from_millis(100);
    LocalSet::new().block_on(&rt, async {
        let _server: tokio::task::JoinHandle<()> =
            tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config().with_timeout(timeout), Anonymous, 8);
        client.connect().await?;
        stalled.set(true);
        let begun = tokio::time::timeout(timeout * 10, client.begin(0, true))
            .await
            .map_err(|_| TestError::new("the Begin's write was not bounded by the timeout"))?;
        assert!(begun.is_err(), "the Begin fails with its connection");
        assert!(!client.core().is_connected());
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A stream over the simulated network that, once `cut` is set to `(bytes, hold)`, hands its next read at most `bytes`
/// bytes and then holds every read back for `hold`: a frame arriving in two parts, a pause between them.
#[derive(Debug)]
struct Cutting {
    inner: TokioSimStream,
    cut: Rc<std::cell::Cell<Option<(usize, Duration)>>>,
    held: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl AsyncRead for Cutting {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if let Some(held) = self.held.as_mut() {
            if held.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }
            self.held = None;
        }
        let Some((bytes, hold)) = self.cut.get() else {
            return Pin::new(&mut self.inner).poll_read(cx, buf);
        };
        let mut part = vec![0; bytes.min(buf.remaining())];
        let mut limited = ReadBuf::new(&mut part);
        let read = Pin::new(&mut self.inner).poll_read(cx, &mut limited);
        if read.is_ready() {
            buf.put_slice(limited.filled());
            self.cut.set(None);
            self.held = Some(Box::pin(tokio::time::sleep(hold)));
        }
        read
    }
}

impl AsyncWrite for Cutting {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Connections over the simulated network whose reads are cut as `cut` says.
struct CutConnect {
    sim: SimConnect,
    cut: Rc<std::cell::Cell<Option<(usize, Duration)>>>,
}

impl Connect for CutConnect {
    type Stream = Cutting;

    async fn connect(&mut self) -> io::Result<Cutting> {
        Ok(Cutting {
            inner: self.sim.connect().await?,
            cut: Rc::clone(&self.cut),
            held: None,
        })
    }
}

/// A deadline falling while a frame is half read leaves the frame as far as it was read: the renewal due goes out
/// between the frame's two parts, and the read reads on from where it stood, on the same connection.
#[test]
fn test_a_frame_cut_by_a_deadline_reads_on_from_where_it_stood() -> TestResult {
    use ultragraphdb_client_core::{TEST_NAME, TEST_PASSWORD};
    use ultragraphdb_protocol_scram::{MECHANISM, ScramLogin, system_nonce};
    let rt = runtime()?;
    let results = 3;
    // Renewed at half its life, while the first Result frame is held between its parts.
    let life = Duration::from_millis(400);
    let scripted = Rc::new(std::cell::RefCell::new(
        ultragraphdb_client_core::TestServer::new(
            crate::utils::serve::CLUSTER,
            crate::utils::serve::welcome().with_mechanisms([MECHANISM]),
        )
        .with_credential_life(life)
        .with_results(results),
    ));
    let sim = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&sim.accepted);
    let cut = Rc::new(std::cell::Cell::new(None));
    let connector = CutConnect {
        sim,
        cut: Rc::clone(&cut),
    };
    LocalSet::new().block_on(&rt, async {
        let _server = tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let login = ScramLogin::new(TEST_NAME, TEST_PASSWORD, system_nonce)?;
        let mut client = Client::new(connector, config(), login, 8);
        client.connect().await?;
        let first = client.core().expires().context("an expiry")?;
        let reader = client.begin(0, true).await?;
        // A header's first bytes, then nothing until well past the renewal's time.
        cut.set(Some((16, life)));
        let stream = client.read(&reader, &read(&reader)).await?;
        assert_eq!(batches(&mut client, &stream).await?, results as usize);
        assert!(cut.get().is_none(), "the frame was cut");
        let _session = client.register().await?;
        let renewed = client.core().expires().context("an expiry")?;
        assert!(
            renewed > first,
            "the credential was renewed between the frame's parts"
        );
        assert!(client.core().is_ready());
        Ok::<(), TestError>(())
    })?;
    Ok(())
}
