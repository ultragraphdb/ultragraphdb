/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The minimal server the client is tested against until the node exists (group 10): the protocol's connection
//! rules through the transport's [`ServerConnection`], one fixed answer per request, and the session table that
//! makes a retried commit apply once.

mod test_server_commit;
mod test_server_requests;
mod test_transaction;

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{
    BufferPool, FrameStep, Framer, Inbound, ReceivedFrame, ServerConnection, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{ErrorCode, MAX_FRAME_SIZE, RequestTerm, Welcome};

use crate::utils_test::session_record::SessionRecord;
use crate::utils_test::test_authenticator::TestAuthenticator;
use crate::utils_test::test_server::test_transaction::{
    ScriptedAnswer, ScriptedRead, TestTransaction, WaitingRead,
};

/// A minimal server for the client's tests, sans-I/O as the client is. Each connection is a [`ServerConnection`],
/// which holds the handshake, the Authenticate exchange and its renewals by SCRAM-SHA-256 when the Welcome lists it,
/// the frames' order, the stream rules, credit and cancel. The server answers each request with one fixed reply: a
/// session for Register, a snapshot for Begin, [`TestServer::with_results`] Result frames for a read or an execute,
/// a statement for Prepare, and a commit from its session table, which answers a retry from its record.
///
/// A test injects a fault by name before the request it hits: [`TestServer::drop_after_next_commit`],
/// [`TestServer::lose_next_commit`], [`TestServer::answer_busy_retries`], [`TestServer::answer_busy_registers`],
/// [`TestServer::send_past_credit`], [`TestServer::lose_next_retries`],
/// [`TestServer::evict`], [`TestServer::refuse_next_commit`], [`TestServer::apply_and_refuse_next_commit`],
/// [`TestServer::refuse_next_read_after`]. Reads wait on a token's epoch under [`TestServer::with_closed_epoch`], until
/// [`TestServer::close_epoch`] or their transaction's deadline. Sessions and the commits applied outlive a connection;
/// transactions and statements, which belong to it, do not.
#[derive(Debug)]
pub struct TestServer {
    cluster: u64,
    welcome: Welcome,
    results: u32,
    credential_life: Option<Duration>,
    connection: Option<ServerConnection<TestAuthenticator>>,
    framer: Framer,
    pool: BufferPool,
    // Shared with the connection's authenticator, which reads the server's wall clock; an atomic, since a credential's
    // expiry is read behind the transport's `Authenticator`, which takes no time.
    clock: Arc<AtomicU64>,
    origin: Option<(Instant, u64)>,
    schema: u64,
    drop_after_commit: bool,
    lose_commit: bool,
    busy_retries: u32,
    busy_registers: u32,
    past_credit: bool,
    lost_retries: u32,
    answers: VecDeque<ScriptedAnswer>,
    read_refusal: Option<ScriptedRead>,
    closed: Option<u64>,
    waiting: Vec<WaitingRead>,
    position: Option<u64>,
    now: Option<Instant>,
    sessions: BTreeMap<u128, SessionRecord>,
    transactions: BTreeMap<u64, TestTransaction>,
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
            busy_registers: 0,
            past_credit: false,
            lost_retries: 0,
            answers: VecDeque::new(),
            read_refusal: None,
            closed: None,
            waiting: Vec::new(),
            position: None,
            now: None,
            sessions: BTreeMap::new(),
            transactions: BTreeMap::new(),
            statements: BTreeMap::new(),
            received: Vec::new(),
            applied: 0,
            // Epoch 1's version 998: a reply's token names the epoch of its commit version (`PROTOCOL.md` §6.3).
            version: (1 << 22) | 998,
            dropped: false,
        }
    }

    /// The server answering each read and execute with `results` Result frames, the last flagged; with 0, one flagged
    /// batch of no rows, as a read that matches nothing is answered.
    pub fn with_results(mut self, results: u32) -> Self {
        self.results = results;
        self
    }

    /// The server's credentials lasting `life` from their exchange's end.
    pub fn with_credential_life(mut self, life: Duration) -> Self {
        self.credential_life = Some(life);
        self
    }

    /// The server having closed epochs up to `epoch`, so a read whose transaction carries a token of a later one waits
    /// until [`TestServer::close_epoch`] or its deadline; with none named, every epoch counts as closed.
    #[must_use]
    pub fn with_closed_epoch(mut self, epoch: u64) -> Self {
        self.closed = Some(epoch);
        self
    }

    /// Epochs up to `epoch` close: each read waiting on one is answered, at its epoch.
    pub fn close_epoch(&mut self, epoch: u64) {
        self.closed = Some(epoch);
        let (ready, waiting) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|read| read.epoch <= epoch);
        self.waiting = waiting;
        for read in ready as Vec<WaitingRead> {
            self.results_at(read.stream, read.epoch);
        }
    }

    /// The next commits apply in `epoch`: their versions and tokens name it.
    pub fn set_epoch(&mut self, epoch: u64) {
        self.version = (epoch << 22) | 998;
    }

    /// The next commit's round is at `position` in the partition's log, as its token names it.
    pub fn set_position(&mut self, position: u64) {
        self.position = Some(position);
    }

    /// The next Commit, Drop, DirectWrite or ConditionalWrite, whatever else would answer it, is refused with `code` and
    /// `detail`, applying nothing; each call scripts one more, in order.
    pub fn refuse_next_commit(&mut self, code: ErrorCode, detail: u64) {
        self.answers.push_back(ScriptedAnswer {
            code,
            detail,
            apply: false,
            value: None,
        });
    }

    /// The next Commit, Drop, DirectWrite or ConditionalWrite is applied, and its reply lost to a refusal of `code`: the
    /// partition holds it, as a role connection closing under it leaves it with code 17.
    pub fn apply_and_refuse_next_commit(&mut self, code: ErrorCode, detail: u64) {
        self.answers.push_back(ScriptedAnswer {
            code,
            detail,
            apply: true,
            value: None,
        });
    }

    /// The next ConditionalWrite's condition does not hold: refused with code 8 and `found`, the value found.
    pub fn fail_next_condition(&mut self, found: RequestTerm) {
        self.answers.push_back(ScriptedAnswer {
            code: ErrorCode::OperationFailed,
            detail: 0,
            apply: false,
            value: Some(found),
        });
    }

    /// The next read sends `batches` Result frames, none flagged, and is then refused with `code` and `detail`.
    pub fn refuse_next_read_after(&mut self, batches: u32, code: ErrorCode, detail: u64) {
        self.read_refusal = Some(ScriptedRead {
            batches,
            code,
            detail,
        });
    }

    /// A new connection at `now`, when the server's wall clock reads `unix_micros`: the handshake again, no
    /// transaction, statement or queued frame kept.
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
        self.statements.clear();
        self.waiting.clear();
    }

    /// The next commit in a transaction, or the next Drop, is applied, and the connection dropped before its reply: a
    /// lost reply.
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

    /// The next `registers` Registers are refused with code 14 (SessionBusy), as a full registry refuses one that holds no
    /// session it may evict.
    pub fn answer_busy_registers(&mut self, registers: u32) {
        self.busy_registers = registers;
    }

    /// The next read's answer takes one frame more than its stream's credit: that frame spends the credit of the read's
    /// transaction's stream, and its header names the read's.
    pub fn send_past_credit(&mut self) {
        self.past_credit = true;
    }

    /// The next `retries` retried commits, on streams of their own, are lost with the connection before they are
    /// answered: a retry that arrived and whose reply never left.
    pub fn lose_next_retries(&mut self, retries: u32) {
        self.lost_retries = retries;
    }

    /// `session` is evicted from the table, so its next commit is refused with code 12 (SessionEvicted).
    pub fn evict(&mut self, session: u128) {
        let _evicted: Option<SessionRecord> = self.sessions.remove(&session);
    }

    /// The tenant's schema id changes, as a catalog change changes it.
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

    /// The token the server sends with a commit or a drop answered at `version`, as its replies carry it: what a
    /// test building a reply by hand gives it.
    #[must_use]
    pub fn token(version: u64) -> ultragraphdb_core_ids::SessionToken {
        test_server_commit::token(version)
    }

    /// The version of the last commit applied, or before any the version the server's snapshots read at: what a
    /// Commit, a Drop or a read-only Begin answers, so a test reads it here rather than assuming a number.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The transactions the connection holds open.
    #[must_use]
    pub fn open_transactions(&self) -> usize {
        self.transactions.len()
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
        let read = self.waiting.iter().map(|read| read.deadline).min();
        let connection = self.connection.as_ref()?.next_deadline();
        [read, connection].into_iter().flatten().min()
    }

    /// The time is `now`: a deadline passed closes the connection.
    pub fn handle_timeout(&mut self, now: Instant) {
        self.tick(now);
        let (past, waiting) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|read| read.deadline <= now);
        self.waiting = waiting;
        for read in past as Vec<WaitingRead> {
            self.error(
                read.stream,
                ErrorCode::TimedOut,
                1,
                "a read waited past its transaction's deadline for its epoch",
            );
        }
        let closed = self.connection.as_mut().map(|c| c.handle_timeout(now));
        match closed {
            Some(Ok(Some(Inbound::Closing { .. }))) => self.transactions.clear(),
            // The closing Error could not be built: the connection closes without it.
            Some(Err(_)) => self.close_unsent(),
            Some(Ok(_)) | None => {}
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
                if connection.refuse_connection(&refusal).is_err() {
                    self.close_unsent();
                }
                return;
            }
        };
        let inbound = match connection.receive(&received, now) {
            Ok(inbound) => inbound,
            Err(_) => return self.close_unsent(),
        };
        match inbound {
            Inbound::Welcomed { protocol, .. } => self.framer.set_protocol(protocol),
            Inbound::Request { stream, command } => self.request(stream, command, &received),
            Inbound::Cancelled {
                stream,
                transaction: true,
            }
            | Inbound::Refused { stream, .. } => {
                let _ended: Option<TestTransaction> = self.transactions.remove(&stream);
            }
            Inbound::Closing { .. } => self.transactions.clear(),
            Inbound::Handled | Inbound::Authenticated { .. } | Inbound::Cancelled { .. } => {}
        }
    }

    /// A frame the connection answers with could not be built, so it closes without a last frame: dropped, as the
    /// transport asks of its caller, its transactions with it.
    fn close_unsent(&mut self) {
        self.dropped = true;
        self.transactions.clear();
    }

    /// The server's wall clock at `now`, from the connection's origin.
    fn tick(&mut self, now: Instant) {
        self.now = Some(now);
        if let Some((at, unix)) = self.origin {
            let since =
                u64::try_from(now.saturating_duration_since(at).as_micros()).unwrap_or(u64::MAX);
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
            let Some(wanted) = bytes.get(..want) else {
                // The framer wants a header's 256 bytes or a frame's `size`, a `u32`.
                let size = u32::try_from(want)
                    .map_err(|_| ProtocolError::frame_too_large(want, MAX_FRAME_SIZE))?;
                return Err(ProtocolError::length_mismatch(size, bytes.len()));
            };
            let mut data = buffer.take();
            data.clear();
            data.extend_from_slice(wanted);
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
        TransportErrorEnum::Io { .. }
        | TransportErrorEnum::Closed
        | TransportErrorEnum::Truncated { .. }
        | TransportErrorEnum::PoolExhausted { .. }
        | TransportErrorEnum::StreamZero
        | TransportErrorEnum::StreamOpen { .. }
        | TransportErrorEnum::StreamLimit { .. }
        | TransportErrorEnum::UnknownStream { .. }
        | TransportErrorEnum::Refused { .. }
        | TransportErrorEnum::UnexpectedCommand { .. }
        | TransportErrorEnum::Authentication { .. }
        | TransportErrorEnum::FrameNotBuilt { .. }
        | TransportErrorEnum::Address { .. } => ProtocolError::short_header(0),
    }
}
