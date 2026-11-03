/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod tls_stream_read;
mod tls_stream_write;

use std::io::{self, ErrorKind, Read, Write};
use std::net::TcpStream;

use crate::errors::tls_error::TlsError;
use crate::types::tls_session::TlsSession;

/// The records a read takes from a carrier at once, and the plaintext a record holds at most is less: a TLS record's largest ciphertext, 2^14 bytes and 256 of
/// expansion, with its 5-byte header (RFC 8446 §5.2).
pub const RECORD_SIZE_MAX: usize = (1 << 14) + 256 + 5;

/// The turns, a carrier's writes and one read, a handshake takes at most before a driver refuses it: far more than the few flights of
/// TLS 1.3's or 1.2's handshake, so only a peer trickling bytes reaches it.
pub const HANDSHAKE_TURNS_MAX: usize = 1 << 12;

/// The reads from a carrier a driver's read of plaintext takes at most before it is refused: each takes at least one byte of
/// a record, so far more than a record's worth, read a byte at a time, and the records around it.
pub const READ_TURNS_MAX: usize = 1 << 16;

/// A [`TlsSession`] driven over a blocking stream `S`, a socket unless named otherwise, read and written as any other
/// stream: a read gives the peer's plaintext, a write sends plaintext as records, each moving the session's records
/// through the stream as it goes. The handshake completes when the stream is opened, by
/// [`TlsConnector::connect`](crate::TlsConnector::connect) or [`TlsAcceptor::accept`](crate::TlsAcceptor::accept).
#[derive(Debug)]
pub struct TlsStream<S = TcpStream> {
    session: TlsSession,
    stream: S,
    // Allocated once per connection, as large as one record: what a read takes from the stream.
    incoming: Vec<u8>,
    // Reused for every write: the records waiting for the stream.
    outgoing: Vec<u8>,
}

impl<S: Read + Write> TlsStream<S> {
    /// `session` over `stream`, its handshake completed.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::HandshakeFailed`](crate::TlsErrorEnum::HandshakeFailed) naming the session's host, for a
    /// record the session refused, a stream that failed or closed, or a handshake past [`HANDSHAKE_TURNS_MAX`].
    pub(crate) fn handshake(session: TlsSession, stream: S) -> Result<Self, TlsError> {
        let mut tls = Self {
            session,
            stream,
            incoming: vec![0; RECORD_SIZE_MAX],
            outgoing: vec![0; RECORD_SIZE_MAX],
        };
        let failed = |driver: &Self, reason: String| {
            TlsError::handshake_failed(driver.session.host().to_string(), reason)
        };
        // Each turn sends what the session has, the last flight included, then reads once.
        for _ in 0..HANDSHAKE_TURNS_MAX {
            tls.write_records()
                .map_err(|e| failed(&tls, e.to_string()))?;
            if !tls.session.is_handshaking() {
                debug_assert!(!tls.session.wants_write(), "the last flight went out");
                return Ok(tls);
            }
            match tls.stream.read(&mut tls.incoming) {
                Ok(0) => {
                    return Err(failed(
                        &tls,
                        "the stream closed during the handshake".into(),
                    ));
                }
                Ok(n) => {
                    if let Err(refusal) = tls.receive(n) {
                        // The refusal leaves the session's alert for the peer, sent if the stream takes it; the
                        // error returned is the refusal's, so the write's own is dropped.
                        drop(tls.write_records());
                        return Err(refusal);
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(failed(&tls, e.to_string())),
            }
        }
        Err(failed(
            &tls,
            format!("no handshake after {HANDSHAKE_TURNS_MAX} turns"),
        ))
    }

    /// Gives the session the first `n` bytes the stream read.
    fn receive(&mut self, n: usize) -> Result<(), TlsError> {
        let records = self.incoming.get(..n).unwrap_or_default();
        self.session.receive(records)
    }

    /// Writes every record the session has to the stream, and flushes it.
    pub(crate) fn write_records(&mut self) -> io::Result<()> {
        // Ends: each turn writes at least one byte the session queued, and the session queues nothing meanwhile.
        while self.session.wants_write() {
            let n = self.session.transmit(&mut self.outgoing);
            let records = self.outgoing.get(..n).unwrap_or_default();
            self.stream.write_all(records)?;
        }
        debug_assert!(!self.session.wants_write());
        self.stream.flush()
    }

    /// The session, for its version and state.
    #[must_use]
    pub fn session(&self) -> &TlsSession {
        &self.session
    }

    /// The stream beneath.
    #[must_use]
    pub fn get_ref(&self) -> &S {
        &self.stream
    }

    /// The stream beneath, for its own operations; bytes read or written through it bypass the session.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Reads once from the stream into the session: 0 bytes tell it the stream closed.
    pub(crate) fn read_records(&mut self) -> io::Result<()> {
        let n = self.stream.read(&mut self.incoming)?;
        self.receive(n).map_err(|e| io::Error::other(e.to_string()))
    }
}
