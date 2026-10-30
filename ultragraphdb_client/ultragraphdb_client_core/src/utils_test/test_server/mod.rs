/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The minimal server the client is tested against until the node exists (group 10): the protocol's connection
//! rules through the transport's [`ServerConnection`], one fixed answer per request, and the session table that
//! makes a retried commit apply once.

mod test_server_commit;
mod test_server_requests;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{
    BufferPool, FrameStep, Framer, Inbound, ReceivedFrame, ServerConnection, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{MAX_FRAME_SIZE, Welcome};

use crate::utils_test::session_record::SessionRecord;
use crate::utils_test::test_authenticator::TestAuthenticator;

/// A minimal server for the client's tests, sans-I/O as the client is. Each connection is a [`ServerConnection`],
/// which holds the handshake, the Authenticate exchange and its renewals by SCRAM-SHA-256 when the Welcome lists it,
/// the frames' order, the stream rules, credit and cancel. The server answers each request with one fixed reply: a
/// session for Register, a snapshot for Begin, [`TestServer::with_results`] Result frames for a read or an execute,
/// a statement for Prepare, and a commit from its session table, which answers a retry from its record.
///
/// A test injects a fault by name before the request it hits: [`TestServer::drop_after_next_commit`],
/// [`TestServer::lose_next_commit`], [`TestServer::answer_busy_retries`], [`TestServer::evict`]. Sessions,
/// statements and the commits applied outlive a connection; transactions do not.
#[derive(Debug)]
pub struct TestServer {
    cluster: u64,
    welcome: Welcome,
    results: u32,
    credential_life: Option<Duration>,
    connection: Option<ServerConnection<TestAuthenticator>>,
    framer: Framer,
    pool: BufferPool,
    clock: Arc<AtomicU64>,
    origin: Option<(Instant, u64)>,
    schema: u64,
    drop_after_commit: bool,
    lose_commit: bool,
    busy_retries: u32,
    sessions: BTreeMap<u64, SessionRecord>,
    transactions: BTreeMap<u64, u32>,
    statements: BTreeMap<u64, u64>,
    received: Vec<Vec<u8>>,
    applied: u64,
    version: u64,
    dropped: bool,
}

impl TestServer {
    /// A server of `cluster` welcoming as `welcome`, three Result frames a read, schema id 1, credentials that never
    /// expire; it serves once [`TestServer::connection`] opens a connection.
    pub fn new(cluster: u64, welcome: Welcome) -> Self {
        Self {
            cluster,
            framer: Framer::from_client(cluster, welcome.max_frame() as usize),
            welcome,
            results: 3,
            credential_life: None,
            connection: None,
            pool: BufferPool::new(4, MAX_FRAME_SIZE),
            clock: Arc::new(AtomicU64::new(0)),
            origin: None,
            schema: 1,
            drop_after_commit: false,
            lose_commit: false,
            busy_retries: 0,
            sessions: BTreeMap::new(),
            transactions: BTreeMap::new(),
            statements: BTreeMap::new(),
            received: Vec::new(),
            applied: 0,
            version: 998,
            dropped: false,
        }
    }

    /// The server answering each read and execute with `results` Result frames, the last flagged.
    pub fn with_results(mut self, results: u32) -> Self {
        self.results = results;
        self
    }

    /// The server's credentials lasting `life` from their exchange's end.
    pub fn with_credential_life(mut self, life: Duration) -> Self {
        self.credential_life = Some(life);
        self
    }

    /// A new connection at `now`, when the server's wall clock reads `unix_micros`: the handshake again, no
    /// transaction or queued frame kept.
    pub fn connection(&mut self, now: Instant, unix_micros: u64) {
        self.dropped = false;
        self.origin = Some((now, unix_micros));
        self.clock.store(unix_micros, Ordering::Relaxed);
        self.framer = Framer::from_client(self.cluster, self.welcome.max_frame() as usize);
        let authenticator = TestAuthenticator::new(Arc::clone(&self.clock), self.credential_life);
        self.connection = Some(ServerConnection::new(
            self.cluster,
            self.welcome.clone(),
            authenticator,
            now,
        ));
        self.transactions.clear();
    }

    /// The next commit in a transaction is applied, and the connection dropped before its reply: a lost reply.
    pub fn drop_after_next_commit(&mut self) {
        self.drop_after_commit = true;
    }

    /// The next commit in a transaction is lost with the connection before the commit step: not applied, so its
    /// retry is refused with code 18 (NotCommitted).
    pub fn lose_next_commit(&mut self) {
        self.lose_commit = true;
    }

    /// The next `retries` retried commits, on streams of their own, are refused with code 14 (SessionBusy), as a
    /// retry is while its original is still in the commit step.
    pub fn answer_busy_retries(&mut self, retries: u32) {
        self.busy_retries = retries;
    }

    /// `session` is evicted from the table, so its next commit is refused with code 12 (SessionEvicted).
    pub fn evict(&mut self, session: u64) {
        self.sessions.remove(&session);
    }

    /// The namespace's schema id changes, as a catalog change changes it.
    pub fn set_schema(&mut self, schema: u64) {
        self.schema = schema;
    }

    /// Whether the server dropped the connection, after a commit it dropped or lost.
    pub fn dropped(&self) -> bool {
        self.dropped
    }

    /// Whether the connection closes: its last frame queued, nothing more read.
    pub fn is_closing(&self) -> bool {
        self.connection.as_ref().is_some_and(|c| c.is_closing())
    }

    /// The principal the connection acts for, once it authenticated.
    pub fn principal(&self) -> Option<u64> {
        self.connection.as_ref().and_then(|c| c.principal())
    }

    /// The commits applied, each once.
    pub fn applied(&self) -> u64 {
        self.applied
    }

    /// Every frame received, as its bytes.
    pub fn received(&self) -> &[Vec<u8>] {
        &self.received
    }

    /// The frames the connection numbered so far, so a test's own frame of the server takes the next sequence.
    pub fn sent(&mut self) -> u128 {
        self.connection.as_mut().map_or(0, |c| c.outbound().sent())
    }

    /// The next frame to send, numbered as it leaves.
    pub fn transmit(&mut self) -> Option<Vec<u8>> {
        if self.dropped {
            return None;
        }
        self.connection
            .as_mut()?
            .next_frame()
            .map(|(_, frame)| frame)
    }

    /// When the connection's next deadline passes: its authentication timeout, its credential's expiry, its idle
    /// limit.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.connection.as_ref()?.next_deadline()
    }

    /// The time is `now`: a deadline passed closes the connection.
    pub fn handle_timeout(&mut self, now: Instant) {
        self.tick(now);
        let closed = self.connection.as_mut().and_then(|c| c.handle_timeout(now));
        if let Some(Inbound::Closing { .. }) = closed {
            self.transactions.clear();
        }
    }

    /// Reads one whole frame of the client at `now`, through the connection's framer into a pooled buffer, and
    /// takes it by the connection's rules; a request is answered.
    pub fn receive(&mut self, frame: &[u8], now: Instant) {
        if self.dropped || self.connection.is_none() {
            return;
        }
        self.received.push(frame.to_vec());
        self.tick(now);
        let read = self.read(frame);
        let Some(connection) = self.connection.as_mut() else {
            return;
        };
        let received = match read {
            Ok(received) => received,
            Err(refusal) => {
                connection.refuse_connection(&refusal);
                return;
            }
        };
        match connection.receive(&received, now) {
            Inbound::Welcomed { protocol } => self.framer.set_protocol(protocol),
            Inbound::Request { stream, command } => self.request(stream, command, &received),
            Inbound::Cancelled {
                stream,
                transaction: true,
            }
            | Inbound::Refused { stream, .. } => {
                self.transactions.remove(&stream);
            }
            Inbound::Closing { .. } => self.transactions.clear(),
            _ => {}
        }
    }

    /// The server's wall clock at `now`, from the connection's origin.
    fn tick(&mut self, now: Instant) {
        if let Some((at, unix)) = self.origin {
            let since = now.saturating_duration_since(at).as_micros() as u64;
            self.clock
                .store(unix.saturating_add(since), Ordering::Relaxed);
        }
    }

    /// `bytes`, one whole frame, through the framer; a header rule broken is the framer's refusal.
    fn read(&mut self, bytes: &[u8]) -> Result<ReceivedFrame, ProtocolError> {
        let mut buffer = self.pool.acquire().map_err(rule)?;
        self.framer.reset();
        loop {
            let want = self.framer.want();
            if want > bytes.len() {
                return Err(ProtocolError::LengthMismatch(want as u32, bytes.len()));
            }
            let mut data = buffer.take();
            data.clear();
            data.extend_from_slice(&bytes[..want]);
            buffer.restore(data);
            if let FrameStep::Frame = self.framer.advance(&buffer).map_err(rule)? {
                return Ok(self.framer.finish(buffer));
            }
        }
    }
}

/// A framer's or pool's refusal as the rule it broke; a pool's own failure, which a test's pool of free buffers
/// never has, as a short header.
fn rule(error: ultragraphdb_protocol_transport::TransportError) -> ProtocolError {
    match error.0 {
        TransportErrorEnum::Protocol { error } => error,
        _ => ProtocolError::ShortHeader(0),
    }
}
