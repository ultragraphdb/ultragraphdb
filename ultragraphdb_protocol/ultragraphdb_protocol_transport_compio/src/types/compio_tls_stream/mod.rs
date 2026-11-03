/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod compio_tls_stream_async_read;
mod compio_tls_stream_async_write;

use std::io;

use compio::BufResult;
use compio::buf::{IntoInner, IoBuf};
use compio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use ultragraphdb_transport_tls::{
    HANDSHAKE_TURNS_MAX, RECORD_SIZE_MAX, TlsAcceptor, TlsConnector, TlsError, TlsSession,
};

/// A [`TlsSession`] driven over a compio stream `S`, a [`Carrier`](crate::Carrier) or any other, read and written as
/// a compio stream: a read gives the peer's plaintext, a write sends plaintext as records. The handshake completes
/// when it is opened, by [`CompioTlsStream::connect`] or [`CompioTlsStream::accept`], before the caller reads or
/// writes a byte. The session's code is the one a blocking socket drives (`ultragraphdb_transport_tls::TlsStream`).
///
/// A read or a write dropped at an `.await` leaves the session whole: records not yet written stay queued in it, and
/// bytes read but not yet given to it are lost, which fails the session's next record, as a cut connection would.
#[derive(Debug)]
pub struct CompioTlsStream<S> {
    session: TlsSession,
    stream: S,
    // Allocated once per connection, as large as one record: what a read takes from the stream.
    incoming: Vec<u8>,
    // Allocated once per connection, as large as one record: the records written to the stream.
    outgoing: Vec<u8>,
    // The plaintext a read took from the session, handed to the caller's buffer.
    plaintext: Vec<u8>,
}

impl<S: AsyncRead + AsyncWrite> CompioTlsStream<S> {
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
    /// As [`TlsAcceptor::session`], and [`TlsErrorEnum::HandshakeFailed`](ultragraphdb_transport_tls::TlsErrorEnum),
    /// its host empty, for a handshake that did not complete, such as a client offering no TLS 1.3.
    pub async fn accept(acceptor: &TlsAcceptor, stream: S) -> Result<Self, TlsError> {
        Self::handshake(acceptor.session()?, stream).await
    }

    /// `session` over `stream`, its handshake completed: each turn sends what the session has, the last flight
    /// included, then reads once.
    async fn handshake(session: TlsSession, stream: S) -> Result<Self, TlsError> {
        let mut tls = Self {
            session,
            stream,
            incoming: Vec::with_capacity(RECORD_SIZE_MAX),
            outgoing: vec![0; RECORD_SIZE_MAX],
            plaintext: vec![0; RECORD_SIZE_MAX],
        };
        for _ in 0..HANDSHAKE_TURNS_MAX {
            if let Err(e) = tls.write_records().await {
                return Err(tls.failed(e.to_string()));
            }
            if !tls.session.is_handshaking() {
                debug_assert!(!tls.session.wants_write(), "the last flight went out");
                return Ok(tls);
            }
            match tls.read_stream().await {
                Ok(0) => return Err(tls.failed("the stream closed during the handshake".into())),
                Ok(_) => {}
                Err(e) => return Err(tls.failed(e.to_string())),
            }
            if let Err(refusal) = tls.session.receive(&tls.incoming) {
                // The refusal leaves the session's alert for the peer, sent if the stream takes it; the error
                // returned is the refusal's, so the write's own is dropped.
                drop(tls.write_records().await);
                return Err(refusal);
            }
        }
        Err(tls.failed(format!("no handshake after {HANDSHAKE_TURNS_MAX} turns")))
    }

    /// A failed handshake of `reason`, naming the session's host.
    fn failed(&self, reason: String) -> TlsError {
        TlsError::handshake_failed(self.session.host().to_string(), reason)
    }

    /// Reads once from the stream into `incoming`, which holds what was read: 0 bytes once the stream closed.
    async fn read_stream(&mut self) -> io::Result<usize> {
        let mut incoming = std::mem::take(&mut self.incoming);
        incoming.clear();
        if incoming.capacity() < RECORD_SIZE_MAX {
            incoming.reserve_exact(RECORD_SIZE_MAX);
        }
        let BufResult(read, incoming) = self.stream.read(incoming).await;
        self.incoming = incoming;
        read
    }

    /// Reads once from the stream into the session: 0 bytes tell it the stream closed.
    pub(crate) async fn read_records(&mut self) -> io::Result<()> {
        let _read: usize = self.read_stream().await?;
        self.session
            .receive(&self.incoming)
            .map_err(|e| io::Error::other(e.to_string()))
    }

    /// Writes every record the session has to the stream, and flushes it.
    pub(crate) async fn write_records(&mut self) -> io::Result<()> {
        // Ends: each turn writes at least one byte the session queued, and the session queues nothing meanwhile.
        while self.session.wants_write() {
            let mut outgoing = std::mem::take(&mut self.outgoing);
            // A write dropped at its `.await` took the buffer with it.
            outgoing.resize(RECORD_SIZE_MAX, 0);
            let n = self.session.transmit(&mut outgoing);
            let BufResult(written, records) = self.stream.write_all(outgoing.slice(..n)).await;
            self.outgoing = records.into_inner();
            written?;
        }
        debug_assert!(!self.session.wants_write());
        self.stream.flush().await
    }

    /// Copies the session's plaintext, at most `room` bytes, into `plaintext`.
    pub(crate) fn take_plaintext(&mut self, room: usize) -> ultragraphdb_transport_tls::Plaintext {
        let want = room.min(self.plaintext.len());
        let into = self.plaintext.get_mut(..want).unwrap_or_default();
        self.session.read(into)
    }

    /// The first `n` bytes [`Self::take_plaintext`] copied.
    pub(crate) fn plaintext(&self, n: usize) -> &[u8] {
        self.plaintext.get(..n).unwrap_or_default()
    }

    /// The session, for its plaintext and its close.
    pub(crate) fn session_mut(&mut self) -> &mut TlsSession {
        &mut self.session
    }

    /// The stream beneath, to shut down.
    pub(crate) fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    /// The session, for its version and state.
    pub fn session(&self) -> &TlsSession {
        &self.session
    }

    /// The stream beneath.
    pub fn get_ref(&self) -> &S {
        &self.stream
    }
}
