/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod test_peer_read;
mod test_peer_write;

use std::io::{self, Read, Write};

use crate::types::plaintext::Plaintext;
use crate::types::tls_session::TlsSession;

/// The plaintext the peer moves to its application at once.
const PLAINTEXT_CHUNK: usize = 4096;

/// The other side of a test connection, in memory: a [`TlsSession`], a server presenting [`TestPki`](crate::TestPki)'s
/// certificate or a client trusting its authority, driven by the tested side's own reads and writes, in front of
/// `application`, which receives the decrypted bytes and answers. The tested side writes its TLS records into the
/// peer and reads the peer's from it, so a handshake and an exchange run on the test's own thread, with no socket.
#[derive(Debug)]
pub struct TestPeer<A> {
    session: TlsSession,
    application: A,
    failed: bool,
}

impl<A: Read + Write> TestPeer<A> {
    /// `session` in front of `application`.
    pub fn new(session: TlsSession, application: A) -> Self {
        Self {
            session,
            application,
            failed: false,
        }
    }

    /// The application behind the peer, for what it received.
    pub fn application(&self) -> &A {
        &self.application
    }

    /// The peer's session, for its version and state.
    pub fn session(&self) -> &TlsSession {
        &self.session
    }

    /// Whether the peer refused the connection: a record it could not take, or the other side's alert.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Takes the other side's TLS records in `records`, then lets the application answer what they carried.
    fn receive(&mut self, records: &[u8]) -> io::Result<()> {
        if self.failed || records.is_empty() {
            return Ok(());
        }
        // A refused record leaves an alert queued for the other side, which `send` passes on.
        self.failed = self.session.receive(records).is_err();
        self.answer()
    }

    /// Moves the decrypted bytes to the application and the application's answer to the session.
    fn answer(&mut self) -> io::Result<()> {
        if self.failed {
            return Ok(());
        }
        let mut chunk = [0u8; PLAINTEXT_CHUNK];
        // Ends: each turn moves at least one byte the session decrypted, and the session holds what `records` carried.
        while let Plaintext::Read(n) = self.session.read(&mut chunk) {
            self.application
                .write_all(chunk.get(..n).unwrap_or_default())?;
        }
        let mut answer = Vec::new();
        let _answered: usize = self.application.read_to_end(&mut answer)?;
        if answer.is_empty() {
            return Ok(());
        }
        self.session
            .send(&answer)
            .map_err(|e| io::Error::other(e.to_string()))
    }

    /// The peer's TLS records waiting for the other side, as many as fit `buf`; 0 once it has none.
    fn send(&mut self, buf: &mut [u8]) -> usize {
        self.session.transmit(buf)
    }
}
