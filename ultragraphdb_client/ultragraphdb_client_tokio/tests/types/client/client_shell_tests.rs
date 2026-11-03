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

use ultragraphdb_client_tokio::{
    Anonymous, Client, ClientError, ClientErrorEnum, Connect, TokioSimStream,
};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork, TransportError};

use crate::utils::serve::{SimConnect, config, serve_sim, server};
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
        assert_eq!(
            exhausted.0,
            ClientErrorEnum::RetriesExhausted {
                session: session.number(),
                request: 1,
                attempts: 4
            }
        );
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
/// reconnect, so the retries yield and an outer timeout ends them, and a connection lost in its handshake uses none of
/// the commit's attempts, two of which would otherwise be spent at once.
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
            .with_commit_attempts(2)
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
