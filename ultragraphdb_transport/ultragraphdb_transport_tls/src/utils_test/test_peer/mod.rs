/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod test_peer_read;
mod test_peer_write;

use std::io::{self, Read, Write};

use rustls::ServerConnection;

/// The server side of a test connection, in memory: a rustls server presenting [`TestPki`](crate::TestPki)'s
/// certificate, driven by the client's own reads and writes, in front of `application`, which receives the
/// decrypted bytes and answers. A client writes its TLS records into the peer and reads the server's from it, so a
/// handshake and an exchange run on the test's own thread, with no socket.
#[derive(Debug)]
pub struct TestPeer<A> {
    server: ServerConnection,
    application: A,
    failed: bool,
}

impl<A: Read + Write> TestPeer<A> {
    pub(crate) fn new(server: ServerConnection, application: A) -> Self {
        Self {
            server,
            application,
            failed: false,
        }
    }

    /// The application behind the server, for what it received.
    pub fn application(&self) -> &A {
        &self.application
    }

    /// Whether the server refused the connection: a record it could not take, or the client's alert.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Takes the client's TLS records in `records`, then lets the application answer what they carried.
    fn receive(&mut self, mut records: &[u8]) -> io::Result<()> {
        while !self.failed && !records.is_empty() {
            if self.server.read_tls(&mut records)? == 0 {
                break;
            }
            // A refused record leaves an alert queued for the client, which `send` passes on.
            self.failed = self.server.process_new_packets().is_err();
        }
        self.answer()
    }

    /// Moves the decrypted bytes to the application and the application's answer to the server.
    fn answer(&mut self) -> io::Result<()> {
        if self.failed {
            return Ok(());
        }
        let mut plaintext = Vec::new();
        match self.server.reader().read_to_end(&mut plaintext) {
            Ok(_) => {}
            // rustls has no more plaintext for now; what it had is in `plaintext`.
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
        self.application.write_all(&plaintext)?;
        let mut answer = Vec::new();
        let _answered: usize = self.application.read_to_end(&mut answer)?;
        self.server.writer().write_all(&answer)
    }

    /// The server's TLS records waiting for the client, as many as fit `buf`; 0 once it has none.
    fn send(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.server.wants_write() {
            return Ok(0);
        }
        let mut out: &mut [u8] = buf;
        self.server.write_tls(&mut out)
    }
}
