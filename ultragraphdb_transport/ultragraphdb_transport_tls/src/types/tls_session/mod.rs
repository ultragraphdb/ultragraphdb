/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{ErrorKind, Read, Write};

use rustls::{Connection, ProtocolVersion};

use crate::errors::tls_error::TlsError;
use crate::types::plaintext::Plaintext;

/// One TLS connection, client or server, with no I/O of its own: it takes the bytes its carrier read
/// ([`TlsSession::receive`]) and gives the bytes to write to it ([`TlsSession::transmit`]), so a blocking socket
/// ([`TlsStream`](crate::TlsStream)), the compio transport and the tokio client shell drive the same handshake and
/// records. Plaintext goes in through [`TlsSession::send`] and comes out through [`TlsSession::read`].
///
/// A connector ([`TlsConnector::session`](crate::TlsConnector::session)) or an acceptor
/// ([`TlsAcceptor::session`](crate::TlsAcceptor::session)) opens it.
#[derive(Debug)]
pub struct TlsSession {
    connection: Connection,
    host: String,
    // Whether the session has ended: its close_notify queued, the peer's received, or a record refused.
    ended: bool,
}

impl TlsSession {
    /// The session of `connection`, with `host` its peer: the name a client verifies, empty on the accepting side.
    pub(crate) fn new(mut connection: Connection, host: String) -> Self {
        debug_assert!(connection.is_handshaking());
        // No limit of rustls's own: a limit makes `send` fail past 64 KiB, a frame reaches 1 MiB, and a carrier writes
        // what a `send` queued before the next, which is what bounds it.
        connection.set_buffer_limit(None);
        Self {
            connection,
            host,
            ended: false,
        }
    }

    /// Takes `records`, bytes the carrier read, and processes every whole record among them; an empty `records` says
    /// the carrier closed.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::HandshakeFailed`](crate::TlsErrorEnum::HandshakeFailed) for a record refused during the
    /// handshake, such as an untrusted certificate or a protocol version the other side does not speak, and
    /// [`RecordRefused`](crate::TlsErrorEnum::RecordRefused) for one refused after it. Either way the session queues
    /// its alert for the peer, which [`TlsSession::transmit`] still gives, and has ended: it sends nothing more.
    pub fn receive(&mut self, mut records: &[u8]) -> Result<(), TlsError> {
        if records.is_empty() {
            // A read of nothing is how rustls learns that its carrier closed.
            let closed = self.connection.read_tls(&mut records);
            debug_assert!(matches!(closed, Ok(0)), "{closed:?}");
            return self.process();
        }
        // Ends: reading from a slice that is not empty takes at least one byte of it, and rustls refuses rather than
        // takes none, since `process` empties what it buffered on every turn.
        while !records.is_empty() {
            if let Err(e) = self.connection.read_tls(&mut records) {
                self.ended = true;
                return Err(self.refused(e.to_string()));
            }
            self.process()?;
        }
        Ok(())
    }

    /// Processes the records taken: a refusal names the handshake or the record it broke, and ends the session, as
    /// the peer's close_notify does.
    fn process(&mut self) -> Result<(), TlsError> {
        match self.connection.process_new_packets() {
            Ok(state) => {
                if state.peer_has_closed() {
                    self.ended = true;
                }
                Ok(())
            }
            Err(error) => {
                self.ended = true;
                Err(self.refused(error.to_string()))
            }
        }
    }

    /// The error of `reason`: a failed handshake while one is under way, a refused record after it.
    fn refused(&self, reason: String) -> TlsError {
        if self.connection.is_handshaking() {
            TlsError::handshake_failed(self.host.clone(), reason)
        } else {
            TlsError::record_refused(self.host.clone(), reason)
        }
    }

    /// Copies the records waiting for the carrier into `out`, as many bytes as fit; the bytes copied, 0 when none
    /// waits. What does not fit waits for the next call.
    pub fn transmit(&mut self, mut out: &mut [u8]) -> usize {
        let room = out.len();
        // Writing into a slice fails only once it is full, which ends the call as well.
        let written = self.connection.write_tls(&mut out).unwrap_or(0);
        debug_assert!(written <= room);
        written
    }

    /// Queues `plaintext` for the peer, encrypted once the handshake completes; [`TlsSession::transmit`] gives it.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::RecordRefused`](crate::TlsErrorEnum::RecordRefused) once the session has ended: its own
    /// close_notify queued ([`TlsSession::close`]), the peer's received, or a record refused by
    /// [`TlsSession::receive`].
    pub fn send(&mut self, plaintext: &[u8]) -> Result<(), TlsError> {
        if self.ended {
            return Err(TlsError::record_refused(
                self.host.clone(),
                "the session has ended".to_string(),
            ));
        }
        self.connection
            .writer()
            .write_all(plaintext)
            .map_err(|e| TlsError::record_refused(self.host.clone(), e.to_string()))
    }

    /// Copies the peer's plaintext received so far into `buf`, as much as fits.
    pub fn read(&mut self, buf: &mut [u8]) -> Plaintext {
        if buf.is_empty() {
            return Plaintext::Pending;
        }
        match self.connection.reader().read(buf) {
            Ok(0) => Plaintext::Closed,
            Ok(n) => Plaintext::Read(n),
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => Plaintext::Truncated,
            // `WouldBlock`, rustls's only other answer: nothing decrypted yet.
            Err(_) => Plaintext::Pending,
        }
    }

    /// Queues the `close_notify` alert, which ends the session for the peer and for [`TlsSession::send`].
    pub fn close(&mut self) {
        self.ended = true;
        self.connection.send_close_notify();
    }

    /// Whether the handshake is still under way.
    #[must_use]
    pub fn is_handshaking(&self) -> bool {
        self.connection.is_handshaking()
    }

    /// Whether records wait for the carrier.
    #[must_use]
    pub fn wants_write(&self) -> bool {
        self.connection.wants_write()
    }

    /// Whether the session waits for records from the carrier.
    #[must_use]
    pub fn wants_read(&self) -> bool {
        self.connection.wants_read()
    }

    /// The TLS version agreed, once the handshake has chosen one.
    #[must_use]
    pub fn protocol_version(&self) -> Option<ProtocolVersion> {
        self.connection.protocol_version()
    }

    /// The peer: the host a client verifies the server for, empty on the accepting side.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }
}
