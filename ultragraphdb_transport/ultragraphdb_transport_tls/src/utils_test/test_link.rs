/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{ErrorKind, Read, Write};

use crate::errors::tls_error::{TlsError, TlsErrorEnum};
use crate::types::plaintext::Plaintext;
use crate::types::tls_session::TlsSession;

/// The pumps an exchange takes at most: far more than a handshake's flights and a line each way.
const TURNS_MAX: usize = 1_000;

/// The bytes a pump moves at once.
const CHUNK: usize = 1 << 15;

/// A [`TlsSession`] and the non-blocking carrier it runs over, an in-memory [`TestPipe`](crate::TestPipe) or a socket,
/// pumped by the test: the same session code over either, since the session does no I/O of its own.
#[derive(Debug)]
pub struct TestLink<C> {
    session: TlsSession,
    carrier: C,
    chunk: Vec<u8>,
}

impl<C: Read + Write> TestLink<C> {
    /// `session` over `carrier`.
    pub fn new(session: TlsSession, carrier: C) -> Self {
        Self {
            session,
            carrier,
            chunk: vec![0; CHUNK],
        }
    }

    /// The session.
    pub fn session(&self) -> &TlsSession {
        &self.session
    }

    /// Writes every record the session has for the carrier, then gives the session what the carrier holds: nothing
    /// when it would block, the close when it reads 0.
    ///
    /// # Errors
    ///
    /// The session's refusal of a record, and [`TlsErrorEnum::ConfigurationRefused`] for a carrier that failed.
    pub fn pump(&mut self) -> Result<(), TlsError> {
        let failed =
            |e: std::io::Error| TlsError(TlsErrorEnum::ConfigurationRefused(e.to_string()));
        // Ends: each turn writes at least one byte the session queued, and the session queues nothing meanwhile.
        while self.session.wants_write() {
            let n = self.session.transmit(&mut self.chunk);
            self.carrier
                .write_all(self.chunk.get(..n).unwrap_or_default())
                .map_err(failed)?;
        }
        match self.carrier.read(&mut self.chunk) {
            Ok(n) => self
                .session
                .receive(self.chunk.get(..n).unwrap_or_default()),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(failed(e)),
        }
    }

    /// Sends `line` from `client` to `server`, which answers with what it received: the bytes that came back to the
    /// client. Both links are pumped in turn until the handshake completes and the line has gone there and back.
    ///
    /// # Errors
    ///
    /// Either side's refusal, and [`TlsErrorEnum::ConfigurationRefused`] when the exchange takes more than its turns.
    pub fn exchange_line(
        client: &mut TestLink<C>,
        server: &mut TestLink<C>,
        line: &[u8],
    ) -> Result<Vec<u8>, TlsError> {
        client.session.send(line)?;
        let mut received = Vec::new();
        let mut answered = Vec::new();
        for _ in 0..TURNS_MAX {
            client.pump()?;
            server.pump()?;
            server.take(&mut received);
            if received.len() == line.len() && answered.is_empty() {
                server.session.send(&received)?;
                answered.clone_from(&received);
            }
            let mut echoed = Vec::new();
            client.take(&mut echoed);
            if !echoed.is_empty() {
                return Ok(echoed);
            }
        }
        Err(TlsError(TlsErrorEnum::ConfigurationRefused(format!(
            "no answer after {TURNS_MAX} turns"
        ))))
    }

    /// Appends the plaintext the session holds to `out`.
    fn take(&mut self, out: &mut Vec<u8>) {
        // Ends: each turn moves at least one byte the session decrypted.
        while let Plaintext::Read(n) = self.session.read(&mut self.chunk) {
            out.extend_from_slice(self.chunk.get(..n).unwrap_or_default());
        }
    }
}
