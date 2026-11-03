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

use crate::utils::serve::{SimConnect, config, serve_sim, server};
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
/// reconnect, so the retries yield and an outer timeout ends them, and a connection lost in its handshake uses none of
/// the commit's attempts, two of which would otherwise be spent at once.
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
            .with_commit_attempts(2)
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
