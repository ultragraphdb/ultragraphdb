/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod tokio_tls_stream_async_read;
mod tokio_tls_stream_async_write;

use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use ultragraphdb_transport_tls::{
    HANDSHAKE_TURNS_MAX, RECORD_SIZE_MAX, TlsAcceptor, TlsConnector, TlsError, TlsSession,
};

/// A [`TlsSession`] driven over a tokio stream `S`, read and written as a tokio stream: a read gives the peer's plaintext,
/// a write sends plaintext as records. The handshake completes when it is opened, by [`TokioTlsStream::connect`] or
/// [`TokioTlsStream::accept`], before the caller reads or writes a byte. The session's code is the one a blocking socket
/// and the compio transport drive.
///
/// Every poll keeps what it has done, so a read or a write dropped between polls leaves the stream whole.
#[derive(Debug)]
pub struct TokioTlsStream<S> {
    session: TlsSession,
    stream: S,
    // Allocated once per connection, as large as one record: what a read takes from the stream.
    incoming: Vec<u8>,
    // Allocated once per connection, as large as one record: the records being written, from `written` to `queued`.
    outgoing: Vec<u8>,
    written: usize,
    queued: usize,
}

impl<S: AsyncRead + AsyncWrite + Unpin> TokioTlsStream<S> {
    /// `stream` as a client connection verified by `connector` for `host`, its handshake completed.
    ///
    /// # Errors
    ///
    /// As [`TlsConnector::session`], and [`TlsErrorEnum::HandshakeFailed`](ultragraphdb_transport_tls::TlsErrorEnum)
    /// naming `host` for a handshake that did not complete or did not verify.
    pub async fn connect(
        connector: &TlsConnector,
        stream: S,
        host: &str,
    ) -> Result<Self, TlsError> {
        Self::handshake(connector.session(host)?, stream).await
    }

    /// `stream` as a server connection of `acceptor`, TLS 1.3 only, its handshake completed.
    ///
    /// # Errors
    ///
    /// As [`TlsAcceptor::session`], and [`TlsErrorEnum::HandshakeFailed`](ultragraphdb_transport_tls::TlsErrorEnum), its
    /// host empty, for a handshake that did not complete, such as a client offering no TLS 1.3.
    pub async fn accept(acceptor: &TlsAcceptor, stream: S) -> Result<Self, TlsError> {
        Self::handshake(acceptor.session()?, stream).await
    }

    /// `session` over `stream`, its handshake completed: each turn sends what the session has, the last flight included,
    /// then reads once.
    async fn handshake(session: TlsSession, stream: S) -> Result<Self, TlsError> {
        let mut tls = Self {
            session,
            stream,
            incoming: vec![0; RECORD_SIZE_MAX],
            outgoing: vec![0; RECORD_SIZE_MAX],
            written: 0,
            queued: 0,
        };
        for _ in 0..HANDSHAKE_TURNS_MAX {
            if let Err(e) = poll_fn(|cx| tls.poll_write_records(cx)).await {
                return Err(tls.failed(e.to_string()));
            }
            if !tls.session.is_handshaking() {
                debug_assert!(!tls.session.wants_write(), "the last flight went out");
                return Ok(tls);
            }
            match poll_fn(|cx| tls.poll_read_records(cx)).await {
                Ok(()) => {}
                // A read a signal interrupted is retried, as the blocking stream's handshake retries it; the turn
                // counts, and nothing was read for the session to take.
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    // The refusal leaves the session's alert for the peer, sent if the stream takes it; the error
                    // returned is the refusal's, so the write's own is dropped.
                    drop(poll_fn(|cx| tls.poll_write_records(cx)).await);
                    return Err(tls.failed(e.to_string()));
                }
            }
        }
        Err(tls.failed(format!("no handshake after {HANDSHAKE_TURNS_MAX} turns")))
    }

    /// A failed handshake of `reason`, naming the session's host.
    fn failed(&self, reason: String) -> TlsError {
        TlsError::handshake_failed(self.session.host().to_string(), reason)
    }

    /// Reads once from the stream into the session; 0 bytes tell it the stream closed.
    pub(crate) fn poll_read_records(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut read = ReadBuf::new(&mut self.incoming);
        ready!(Pin::new(&mut self.stream).poll_read(cx, &mut read))?;
        let refused = self.session.receive(read.filled());
        Poll::Ready(refused.map_err(|e| io::Error::other(e.to_string())))
    }

    /// Writes every record the session has to the stream; pending with the rest kept for the next poll.
    pub(crate) fn poll_write_records(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Ends: each turn writes at least one byte the session queued, and the session queues nothing meanwhile.
        loop {
            if self.written == self.queued {
                self.queued = self.session.transmit(&mut self.outgoing);
                self.written = 0;
                if self.queued == 0 {
                    return Poll::Ready(Ok(()));
                }
            }
            let records = self
                .outgoing
                .get(self.written..self.queued)
                .unwrap_or_default();
            let n = ready!(Pin::new(&mut self.stream).poll_write(cx, records))?;
            if n == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            self.written = self.written.saturating_add(n);
        }
    }

    /// The session, for its plaintext and its close.
    pub(crate) fn session_mut(&mut self) -> &mut TlsSession {
        &mut self.session
    }

    /// The stream beneath, to flush and to shut down.
    pub(crate) fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    /// The session, for its version and state.
    #[must_use]
    pub fn session(&self) -> &TlsSession {
        &self.session
    }
}
