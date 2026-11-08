/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A TLS session over a tokio stream that misbehaves as a socket may: a read a signal interrupted is read again, in the
//! handshake and after it, any other failure is the caller's; a flush reaches the stream beneath, and a shutdown sends
//! the session's `close_notify` before it shuts the stream's sending side.

use std::cell::Cell;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf};
use tokio::runtime::Runtime;
use ultragraphdb_client_tokio::TokioTlsStream;
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};
use ultragraphdb_transport_tls::TestPki;

/// The most any step of these tests waits.
const WAIT: Duration = Duration::from_secs(5);

fn runtime() -> TestResult<Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("a runtime")
}

/// What a [`Probe`] was asked to do, shared with the test that gave the probe away.
#[derive(Debug, Default)]
struct Probed {
    /// The failure the stream's next read gives, once.
    fail_next_read: Cell<Option<io::ErrorKind>>,
    flushes: Cell<u32>,
    shutdowns: Cell<u32>,
}

/// One end of an in-memory stream that fails a read when told to and counts its flushes and shutdowns.
#[derive(Debug)]
struct Probe {
    inner: DuplexStream,
    probed: Rc<Probed>,
}

impl AsyncRead for Probe {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if let Some(kind) = self.probed.fail_next_read.take() {
            return Poll::Ready(Err(kind.into()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Probe {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.probed.flushes.set(self.probed.flushes.get() + 1);
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.probed.shutdowns.set(self.probed.shutdowns.get() + 1);
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// A client session over a [`Probe`] and the server session at the other end, both handshaken, the probe's first read
/// failing with `first_read` when one is given.
async fn pair(
    first_read: Option<io::ErrorKind>,
) -> TestResult<(
    TokioTlsStream<Probe>,
    TokioTlsStream<DuplexStream>,
    Rc<Probed>,
)> {
    let pki = TestPki::new()?;
    let (client_end, server_end) = tokio::io::duplex(1 << 16);
    let probed = Rc::new(Probed::default());
    probed.fail_next_read.set(first_read);
    let probe = Probe {
        inner: client_end,
        probed: Rc::clone(&probed),
    };
    let (connector, acceptor) = (pki.connector()?, pki.acceptor()?);
    let (client, server) = tokio::time::timeout(
        WAIT,
        futures::future::join(
            TokioTlsStream::connect(&connector, probe, "localhost"),
            TokioTlsStream::accept(&acceptor, server_end),
        ),
    )
    .await
    .context("the handshakes end")?;
    Ok((client?, server?, probed))
}

#[test]
fn test_a_handshake_reads_again_after_an_interrupted_read() -> TestResult {
    runtime()?.block_on(async {
        let (client, _server, probed) = pair(Some(io::ErrorKind::Interrupted)).await?;
        assert_eq!(
            probed.fail_next_read.get(),
            None,
            "the interrupted read happened"
        );
        assert!(!client.session().is_handshaking());
        Ok(())
    })
}

/// The server's `ping`, read by the client whose next read of the stream fails with `kind`.
async fn read_after(kind: io::ErrorKind) -> TestResult<io::Result<Vec<u8>>> {
    let (mut client, mut server, probed) = pair(None).await?;
    server.write_all(b"ping").await?;
    server.flush().await?;
    probed.fail_next_read.set(Some(kind));
    let mut read = vec![0; 4];
    let got = tokio::time::timeout(WAIT, client.read(&mut read))
        .await
        .context("the read ends")?;
    assert_eq!(
        probed.fail_next_read.get(),
        None,
        "the failing read happened"
    );
    Ok(got.map(|n| read.into_iter().take(n).collect()))
}

#[test]
fn test_a_read_interrupted_by_a_signal_is_read_again() -> TestResult {
    let read = runtime()?.block_on(read_after(io::ErrorKind::Interrupted))?;
    assert_eq!(read?, b"ping".to_vec());
    Ok(())
}

#[test]
fn test_a_failed_read_is_the_callers() -> TestResult {
    let read = runtime()?.block_on(read_after(io::ErrorKind::ConnectionReset))?;
    assert_eq!(
        read.err().map(|e| e.kind()),
        Some(io::ErrorKind::ConnectionReset)
    );
    Ok(())
}

#[test]
fn test_a_flush_reaches_the_stream_beneath() -> TestResult {
    runtime()?.block_on(async {
        let (mut client, mut server, probed) = pair(None).await?;
        let before = probed.flushes.get();
        client.write_all(b"pong").await?;
        client.flush().await?;
        assert_eq!(probed.flushes.get(), before + 1);
        let mut read = vec![0; 4];
        let n = tokio::time::timeout(WAIT, server.read_exact(&mut read))
            .await
            .context("the plaintext arrives")??;
        assert_eq!(n, 4);
        assert_eq!(read, b"pong".to_vec());
        Ok(())
    })
}

#[test]
fn test_a_shutdown_sends_close_notify_and_shuts_the_stream() -> TestResult {
    runtime()?.block_on(async {
        let (mut client, mut server, probed) = pair(None).await?;
        client.shutdown().await?;
        assert_eq!(probed.shutdowns.get(), 1);
        let mut read = vec![0; 4];
        let n = tokio::time::timeout(WAIT, server.read(&mut read))
            .await
            .map_err(|_| TestError::new("no close_notify reached the server"))??;
        assert_eq!(n, 0, "the peer closed cleanly, by its close_notify");
        Ok(())
    })
}
