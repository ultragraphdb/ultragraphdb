/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::rc::Rc;
use std::time::Duration;

use ultragraphdb_client_compio::{Anonymous, Client, ClientError, ClientErrorEnum, Connect};
use ultragraphdb_protocol_transport::{
    NetworkFaults, SimNetwork, TransportError, TransportErrorEnum,
};

use crate::utils::serve::{SimConnect, config, in_doubt, serve_sim, server};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

#[test]
fn test_close_gives_the_connection_up() -> TestResult {
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let scripted = server();
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
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

/// A stream whose every write fails with `BrokenPipe`, and which reads as ended.
#[derive(Debug)]
struct Broken;

impl compio::io::AsyncRead for Broken {
    async fn read<B: compio::buf::IoBufMut>(&mut self, buf: B) -> compio::BufResult<usize, B> {
        compio::BufResult(Ok(0), buf)
    }
}

impl compio::io::AsyncWrite for Broken {
    async fn write<T: compio::buf::IoBuf>(&mut self, buf: T) -> compio::BufResult<usize, T> {
        compio::BufResult(Err(io::ErrorKind::BrokenPipe.into()), buf)
    }

    async fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    async fn shutdown(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Both halves of a [`Broken`] stream are broken alike.
impl compio::io::util::Splittable for Broken {
    type ReadHalf = Self;
    type WriteHalf = Self;

    fn split(self) -> (Self, Self) {
        (Self, Self)
    }
}

/// Connections refused with `refusal`, made over a [`Broken`] stream with `Some(None)`, or never made with `None`.
struct Unreachable {
    refusal: Option<Option<io::ErrorKind>>,
}

impl Connect for Unreachable {
    type Stream = Broken;

    async fn connect(&mut self) -> io::Result<Broken> {
        match self.refusal {
            Some(Some(kind)) => Err(kind.into()),
            Some(None) => Ok(Broken),
            None => std::future::pending().await,
        }
    }
}

/// `client.connect()` through `connector`, with a timeout of `timeout`.
fn connect_through(
    connector: Unreachable,
    timeout: Duration,
) -> TestResult<Result<(), ClientError>> {
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    Ok(rt.block_on(async {
        let mut client = Client::new(connector, config().with_timeout(timeout), Anonymous, 2);
        let connected = client.connect().await;
        assert!(!client.core().is_ready());
        connected
    }))
}

#[test]
fn test_a_refused_connection_is_the_transport_error() -> TestResult {
    let refusal = Some(Some(io::ErrorKind::ConnectionRefused));
    assert_eq!(
        connect_through(Unreachable { refusal }, Duration::from_secs(2))?,
        Err(ClientError::transport(TransportError::io(
            &io::ErrorKind::ConnectionRefused.into()
        )))
    );
    Ok(())
}

#[test]
fn test_a_connection_not_made_within_the_timeout_times_out() -> TestResult {
    assert_eq!(
        connect_through(Unreachable { refusal: None }, Duration::from_millis(20))?,
        Err(ClientError::timed_out())
    );
    Ok(())
}

#[test]
fn test_a_failed_write_fails_the_connection_with_its_error() -> TestResult {
    // The Hello cannot be written: the connection fails with the write's error, as a failed read's.
    let refused = connect_through(
        Unreachable {
            refusal: Some(None),
        },
        Duration::from_secs(2),
    )?
    .err()
    .context("an error")?;
    assert!(
        matches!(&refused.0, ClientErrorEnum::Transport { error } if matches!(error.0, TransportErrorEnum::Io { .. })),
        "{refused:?}"
    );
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
    type Stream = ultragraphdb_protocol_transport_compio::CompioSimStream;

    async fn connect(
        &mut self,
    ) -> io::Result<ultragraphdb_protocol_transport_compio::CompioSimStream> {
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
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = Flaky::new(inner, 3);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
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
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = Flaky::new(inner, 5_000);
    let (made, failing) = (Rc::clone(&connector.made), Rc::clone(&connector.failing));
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
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
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = Flaky::new(inner, u32::MAX);
    let made = Rc::clone(&connector.made);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let config = config()
            .with_commit_attempts(u32::MAX)
            .with_retry_delay(Duration::from_millis(5));
        let mut client = Client::new(connector, config, Anonymous, 8);
        let session = client.register().await?;
        let tx = client.begin(0, false).await?;
        scripted.borrow_mut().drop_after_next_commit();
        let cancelled =
            compio::time::timeout(Duration::from_millis(100), client.commit(tx, session)).await;
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
    type Stream = ultragraphdb_protocol_transport_compio::CompioSimStream;

    async fn connect(
        &mut self,
    ) -> io::Result<ultragraphdb_protocol_transport_compio::CompioSimStream> {
        self.made.set(self.made.get() + 1);
        if self.made.get() == 1 {
            return self.inner.connect().await;
        }
        let (client, server) = self.inner.network.connect();
        server.shutdown();
        Ok(ultragraphdb_protocol_transport_compio::CompioSimStream::new(client))
    }
}

/// A server that accepts connections and hangs up every handshake: the shell waits the retry delay before each
/// reconnect, so the retries of a commit with attempts to spare yield and an outer timeout ends them.
#[test]
fn test_reconnects_after_handshakes_hung_up_wait_the_retry_delay() -> TestResult {
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = Hangup {
        inner,
        made: Rc::new(std::cell::Cell::new(0)),
    };
    let made = Rc::clone(&connector.made);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let config = config()
            .with_commit_attempts(u32::MAX)
            .with_retry_delay(Duration::from_millis(5));
        let mut client = Client::new(connector, config, Anonymous, 8);
        let session = client.register().await?;
        let tx = client.begin(0, false).await?;
        scripted.borrow_mut().drop_after_next_commit();
        let cancelled =
            compio::time::timeout(Duration::from_millis(100), client.commit(tx, session)).await;
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
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let scripted = server();
    let inner = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&inner.accepted);
    let connector = Hangup {
        inner,
        made: Rc::new(std::cell::Cell::new(0)),
    };
    let made = Rc::clone(&connector.made);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let config = config()
            .with_commit_attempts(4)
            .with_retry_delay(Duration::from_millis(5));
        let mut client = Client::new(connector, config, Anonymous, 8);
        let session = client.register().await?;
        let tx = client.begin(0, false).await?;
        scripted.borrow_mut().drop_after_next_commit();
        let exhausted = compio::time::timeout(Duration::from_secs(2), client.commit(tx, session))
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
fn read(tx: &ultragraphdb_client_compio::Transaction) -> ultragraphdb_protocol_wire::Read {
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
async fn batches<C: Connect, L: ultragraphdb_client_compio::Login>(
    client: &mut Client<C, L>,
    stream: &ultragraphdb_client_compio::ResultStream,
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
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let results = 3;
    let scripted = Rc::new(std::cell::RefCell::new(server_of(1).with_results(results)));
    scripted
        .borrow_mut()
        .refuse_next_commit(ultragraphdb_protocol_wire::ErrorCode::SessionBusy, 0);
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
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
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let results = 6;
    let scripted = Rc::new(std::cell::RefCell::new(server_of(4).with_results(results)));
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
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
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
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
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let login = ScramLogin::new(TEST_NAME, TEST_PASSWORD, system_nonce)?;
        let mut client = Client::new(connector, config(), login, 8);
        client.connect().await?;
        let first = client.core().expires().context("an expiry")?;
        let reader = client.begin(0, true).await?;
        // The renewal falls due before the read is sent, so it goes out while the read's first frame is awaited.
        compio::time::sleep(life / 2).await;
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

/// A frame that arrived while no call drove the connection is read before a deadline that passed meanwhile is taken: a
/// Register is sent and its future dropped, a read takes its first batch, and the caller then waits past the timeout
/// before asking for the next, the server having answered everything at once. The read goes on, on the same connection.
#[test]
fn test_a_frame_waiting_is_read_before_a_deadline_passed() -> TestResult {
    use futures::FutureExt as _;
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let results = 3;
    let scripted = Rc::new(std::cell::RefCell::new(server_of(4).with_results(results)));
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    let timeout = Duration::from_millis(100);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config().with_timeout(timeout), Anonymous, 8);
        let reader = client.begin(0, true).await?;
        let stream = client.read(&reader, &read(&reader)).await?;
        let sent = scripted.borrow().received().len();
        // Sent, and dropped at its first wait: its reply is awaited, which runs the timeout.
        assert!(
            client.register().now_or_never().is_none(),
            "the Register waits"
        );
        assert!(
            client.next_batch(&stream, |_| ()).await?.is_some(),
            "the first batch"
        );
        assert!(
            scripted.borrow().received().len() > sent,
            "the Register was sent"
        );
        compio::time::sleep(timeout * 2).await;
        let mut rest = 0;
        while client.next_batch(&stream, |_| ()).await?.is_some() {
            rest += 1;
        }
        assert_eq!(rest + 1, results as usize);
        assert!(client.core().is_ready());
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A stream over the simulated network whose writes stop being taken once `stalled` is set, as a peer that stopped
/// reading leaves them; its halves stall together.
#[derive(Debug)]
struct Stalling {
    inner: ultragraphdb_protocol_transport_compio::CompioSimStream,
    stalled: Rc<std::cell::Cell<bool>>,
}

impl compio::io::AsyncRead for Stalling {
    async fn read<B: compio::buf::IoBufMut>(&mut self, buf: B) -> compio::BufResult<usize, B> {
        self.inner.read(buf).await
    }
}

impl compio::io::AsyncWrite for Stalling {
    async fn write<T: compio::buf::IoBuf>(&mut self, buf: T) -> compio::BufResult<usize, T> {
        if self.stalled.get() {
            std::future::pending::<()>().await;
        }
        self.inner.write(buf).await
    }

    async fn flush(&mut self) -> io::Result<()> {
        self.inner.flush().await
    }

    async fn shutdown(&mut self) -> io::Result<()> {
        if self.stalled.get() {
            std::future::pending::<()>().await;
        }
        self.inner.shutdown().await
    }
}

impl compio::io::util::Splittable for Stalling {
    type ReadHalf = Self;
    type WriteHalf = Self;

    fn split(self) -> (Self, Self) {
        let (read, write) = self.inner.split();
        let stalled = Rc::clone(&self.stalled);
        (
            Self {
                inner: read,
                stalled: Rc::clone(&stalled),
            },
            Self {
                inner: write,
                stalled,
            },
        )
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
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let scripted = server();
    let sim = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&sim.accepted);
    let stalled = Rc::new(std::cell::Cell::new(false));
    let connector = StallConnect {
        sim,
        stalled: Rc::clone(&stalled),
    };
    let timeout = Duration::from_millis(100);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config().with_timeout(timeout), Anonymous, 8);
        client.connect().await?;
        stalled.set(true);
        let begun = compio::time::timeout(timeout * 10, client.begin(0, true))
            .await
            .map_err(|_| TestError::new("the Begin's write was not bounded by the timeout"))?;
        assert!(begun.is_err(), "the Begin fails with its connection");
        assert!(!client.core().is_connected());
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A stream over the simulated network whose writes, once `allowance` is set, take that many bytes more and then nothing
/// until it is cleared: a send buffer that fills in the middle of a frame.
#[derive(Debug)]
struct Trickling {
    inner: ultragraphdb_protocol_transport_compio::CompioSimStream,
    allowance: Rc<std::cell::Cell<Option<usize>>>,
}

impl compio::io::AsyncRead for Trickling {
    async fn read<B: compio::buf::IoBufMut>(&mut self, buf: B) -> compio::BufResult<usize, B> {
        self.inner.read(buf).await
    }
}

impl compio::io::AsyncWrite for Trickling {
    async fn write<T: compio::buf::IoBuf>(&mut self, buf: T) -> compio::BufResult<usize, T> {
        // Ends: the test clears the allowance within its own bound.
        while self.allowance.get() == Some(0) {
            compio::time::sleep(Duration::from_millis(1)).await;
        }
        let Some(left) = self.allowance.get() else {
            return self.inner.write(buf).await;
        };
        let part = left.min(compio::buf::IoBuf::buf_len(&buf));
        let compio::BufResult(wrote, cut) = self
            .inner
            .write(compio::buf::IoBuf::slice(buf, ..part))
            .await;
        if let Ok(wrote) = &wrote {
            self.allowance.set(Some(left.saturating_sub(*wrote)));
        }
        compio::BufResult(wrote, compio::buf::IntoInner::into_inner(cut))
    }

    async fn flush(&mut self) -> io::Result<()> {
        self.inner.flush().await
    }

    async fn shutdown(&mut self) -> io::Result<()> {
        self.inner.shutdown().await
    }
}

impl compio::io::util::Splittable for Trickling {
    type ReadHalf = Self;
    type WriteHalf = Self;

    fn split(self) -> (Self, Self) {
        let (read, write) = self.inner.split();
        let allowance = Rc::clone(&self.allowance);
        (
            Self {
                inner: read,
                allowance: Rc::clone(&allowance),
            },
            Self {
                inner: write,
                allowance,
            },
        )
    }
}

/// Connections over the simulated network whose writes trickle as `allowance` says.
struct TrickleConnect {
    sim: SimConnect,
    allowance: Rc<std::cell::Cell<Option<usize>>>,
}

impl Connect for TrickleConnect {
    type Stream = Trickling;

    async fn connect(&mut self) -> io::Result<Trickling> {
        Ok(Trickling {
            inner: self.sim.connect().await?,
            allowance: Rc::clone(&self.allowance),
        })
    }
}

/// A caller's future dropped while a frame is half written leaves the rest of it for the next call, so the frame after it
/// follows it whole and the connection serves both.
#[test]
fn test_a_frame_cut_by_a_dropped_future_is_written_whole_by_the_next_call() -> TestResult {
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let scripted = server();
    let sim = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&sim.accepted);
    let allowance = Rc::new(std::cell::Cell::new(None));
    let connector = TrickleConnect {
        sim,
        allowance: Rc::clone(&allowance),
    };
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        client.connect().await?;
        allowance.set(Some(10));
        let cut = compio::time::timeout(Duration::from_millis(50), client.register()).await;
        assert!(
            cut.is_err(),
            "the Register's frame waits on the full buffer"
        );
        allowance.set(None);
        let registered = compio::time::timeout(Duration::from_secs(5), client.register())
            .await
            .map_err(|_| TestError::new("the second Register was not answered within 5 s"))?;
        let _session = registered?;
        assert!(client.core().is_ready(), "the connection serves on");
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A read whose frame waits in the socket behind another read's, while the caller held a batch past the read timeout, is
/// not taken for a silent one: the frames already arrived are read before the deadline is taken, and both reads read on. Its
/// 31 s are the floor of the read timeout, on a clock compio does not let a test move.
#[test]
fn test_a_read_whose_frame_waits_behind_anothers_is_not_timed_out() -> TestResult {
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let results = 3;
    let scripted = Rc::new(std::cell::RefCell::new(server_of(1).with_results(results)));
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        let begin = ultragraphdb_protocol_wire::Begin::new(0, 0, true).with_deadline_ms(1);
        let reader = client.begin_with(begin, None).await?;
        let first = client.read(&reader, &read(&reader)).await?;
        // Two batches of the first read, so its credit brings the third ahead of the second read's first.
        for _ in 0..2 {
            client
                .next_batch(&first, |_| ())
                .await?
                .context("a batch of the first read")?;
        }
        let second = client.read(&reader, &read(&reader)).await?;
        // The caller holds its batch past the read timeout's floor of 30 s, driving nothing.
        compio::time::sleep(Duration::from_secs(31)).await;
        assert_eq!(
            batches(&mut client, &first).await?,
            1,
            "the first read's last"
        );
        assert_eq!(
            batches(&mut client, &second).await?,
            results as usize,
            "the second read, its frame read before the deadline was taken"
        );
        assert!(client.core().is_ready());
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A stream over the simulated network that, once `doomed` is set, fails every read with a reset, and whose shutdown is
/// never taken: a connection that broke under its reader, its sending side left hanging.
#[derive(Debug)]
struct Dooming {
    inner: ultragraphdb_protocol_transport_compio::CompioSimStream,
    doomed: Rc<std::cell::Cell<bool>>,
}

impl compio::io::AsyncRead for Dooming {
    async fn read<B: compio::buf::IoBufMut>(&mut self, buf: B) -> compio::BufResult<usize, B> {
        let compio::BufResult(read, buf) = self.inner.read(buf).await;
        if self.doomed.get() {
            return compio::BufResult(Err(io::ErrorKind::ConnectionReset.into()), buf);
        }
        compio::BufResult(read, buf)
    }
}

impl compio::io::AsyncWrite for Dooming {
    async fn write<T: compio::buf::IoBuf>(&mut self, buf: T) -> compio::BufResult<usize, T> {
        self.inner.write(buf).await
    }

    async fn flush(&mut self) -> io::Result<()> {
        self.inner.flush().await
    }

    async fn shutdown(&mut self) -> io::Result<()> {
        if self.doomed.get() {
            std::future::pending::<()>().await;
        }
        self.inner.shutdown().await
    }
}

impl compio::io::util::Splittable for Dooming {
    type ReadHalf = Self;
    type WriteHalf = Self;

    fn split(self) -> (Self, Self) {
        let (read, write) = self.inner.split();
        let doomed = Rc::clone(&self.doomed);
        (
            Self {
                inner: read,
                doomed: Rc::clone(&doomed),
            },
            Self {
                inner: write,
                doomed,
            },
        )
    }
}

/// Connections over the simulated network, the first doomed when `first` is set, every later one sound.
struct DoomConnect {
    sim: SimConnect,
    first: Option<Rc<std::cell::Cell<bool>>>,
}

impl Connect for DoomConnect {
    type Stream = Dooming;

    async fn connect(&mut self) -> io::Result<Dooming> {
        Ok(Dooming {
            inner: self.sim.connect().await?,
            doomed: self
                .first
                .take()
                .unwrap_or_else(|| Rc::new(std::cell::Cell::new(false))),
        })
    }
}

/// Serves every connection `accepted` holds on a task of its own, so one its client left without a shutdown holds up no other.
async fn serve_each(
    server: Rc<std::cell::RefCell<ultragraphdb_client_core::TestServer>>,
    accepted: Rc<
        std::cell::RefCell<std::collections::VecDeque<ultragraphdb_protocol_transport::SimStream>>,
    >,
) {
    let mut serving = Vec::new();
    loop {
        let next = accepted.borrow_mut().pop_front();
        match next {
            Some(stream) => {
                let server = Rc::clone(&server);
                serving.push(compio::runtime::spawn(async move {
                    crate::utils::serve::serve(
                        &server,
                        ultragraphdb_protocol_transport_compio::CompioSimStream::new(stream),
                    )
                    .await;
                }));
            }
            None => compio::time::sleep(Duration::from_millis(1)).await,
        }
    }
}

/// A connection whose read failed is gone, both its halves: the next request connects again at once, waiting on no
/// shutdown of the broken one's sending side.
#[test]
fn test_a_connection_whose_read_failed_is_left_without_a_shutdown() -> TestResult {
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let scripted = server();
    let sim = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&sim.accepted);
    let doomed = Rc::new(std::cell::Cell::new(false));
    let connector = DoomConnect {
        sim,
        first: Some(Rc::clone(&doomed)),
    };
    let timeout = Duration::from_secs(1);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_each(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config().with_timeout(timeout), Anonymous, 8);
        client.connect().await?;
        doomed.set(true);
        let lost = compio::time::timeout(timeout / 2, client.register())
            .await
            .map_err(|_| TestError::new("the Register's failed read did not end it"))?;
        assert!(
            matches!(
                lost.err().context("a refusal")?.0,
                ClientErrorEnum::ConnectionLost { .. }
            ),
            "the read's failure loses the connection"
        );
        let again = compio::time::timeout(timeout / 2, client.register())
            .await
            .map_err(|_| TestError::new("the next Register waited on the broken connection"))?;
        assert!(again.is_ok(), "{again:?}");
        Ok::<(), TestError>(())
    })?;
    Ok(())
}

/// A wait ends at the core's next deadline when that deadline is ahead: each Register answered code 14 goes out again its
/// retry delay after the answer, not a read turn later. Thirty resends 2 ms apart take about 60 ms; held to the shell's
/// 10 ms read turn (`READ_TURN`) each, they would take 300 ms at least, so the bound sits between, at two thirds of that.
#[test]
fn test_a_resend_due_within_a_read_turn_goes_out_at_its_deadline() -> TestResult {
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let busy: u32 = 30;
    let read_turn = Duration::from_millis(10);
    let scripted = server();
    scripted.borrow_mut().answer_busy_registers(busy);
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let config = config()
            .with_commit_attempts(busy + 1)
            .with_retry_delay(Duration::from_millis(2));
        let mut client = Client::new(connector, config, Anonymous, 8);
        client.connect().await?;
        let registered = compio::time::timeout(read_turn * busy * 2 / 3, client.register())
            .await
            .map_err(|_| TestError::new("the resends waited past their deadlines"))?;
        let _session = registered?;
        assert!(client.core().is_ready());
        Ok::<(), TestError>(())
    })?;
    Ok(())
}
