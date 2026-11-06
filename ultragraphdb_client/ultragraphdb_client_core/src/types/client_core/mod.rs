/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod client_core_debug;
mod client_core_receive;
mod client_core_requests;
mod client_core_results;

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{
    ClientAuthentication, ClientHandshake, Login, ReceivedFrame, TransportError,
};
use ultragraphdb_protocol_wire::{BodyBuilder, Command, Commit, Header, Register, Welcome};

use crate::errors::client_error::{ClientError, ClientErrorEnum};
use crate::types::client_config::ClientConfig;
use crate::types::link::Link;
use crate::types::pending::Pending;
use crate::types::pending_state::PendingState;
use crate::types::reply::Reply;
use crate::types::resend::Resend;
use crate::types::result_stream::handle_held;
use crate::types::session::Session;
use crate::types::session_state::SessionState;
use crate::types::stream_state::StreamState;

/// The least a read's timeout runs, the most `scan_stall_ms` holds: a hung compute process is answered with code 17 by the
/// deadline plus that (`PROTOCOL.md` §11.11), so the server speaks first.
const READ_TIMEOUT_MIN: Duration = Duration::from_secs(30);

/// The client, sans-I/O: every rule of the protocol a client follows, and no I/O and no clock. It authenticates
/// through its [`Login`], [`ultragraphdb_protocol_transport::Anonymous`] for a server asking for none.
///
/// A shell connects, then calls [`ClientCore::connect`], writes every frame [`ClientCore::transmit`] gives, hands
/// every frame it reads, whole, to [`ClientCore::receive`] and a failed connection to
/// [`ClientCore::disconnected`], sleeps no longer than [`ClientCore::next_deadline`] and then calls
/// [`ClientCore::handle_timeout`], and connects again whenever [`ClientCore::needs_connection`] says a commit waits
/// to be retried. Requests start with a call naming the time; a request with one reply answers through
/// [`ClientCore::take_reply`], a result stream through [`ClientCore::next_batch`].
///
/// Frames are numbered as they leave, from 1 on each connection, and the server's are taken only in their order.
/// Streams are numbered from 1 on each connection and never reused on it. A transaction is its stream and its reads:
/// its Begin, writes and operations, and its Commit or Abort go on its stream, each read and execute on a stream of
/// its own naming the transaction; when the transaction ends, its reads end with it. Write, Operation and Abort have
/// no reply; a refusal of one ends the transaction's stream and is the answer of its next request.
///
/// A request declares its type by the call that sends it: a read-only transaction ([`ClientCore::begin_with`] and
/// [`ClientCore::read`]), a [`ClientCore::direct_write`], a [`ClientCore::conditional_write`], or a read-write transaction.
/// Only a read whose stream reached `LAST` is an answer: read one to its end, or cancel it, before waiting on a later read
/// behind a write, since that write waits for the open read's `LAST` and only this client's Credit drains it, and read one
/// to its end before a Commit or an Abort, which ends the reads still open (`PROTOCOL.md` §11.2). A read refused after rows
/// has given no answer, whatever batches it handed over, and nothing of it is resent.
///
/// A session has one request counter, for commits, drops and one-frame writes alike. A request whose frame never left on
/// any connection is lost ([`ClientError::connection_lost`]); once it left, or when answered code 17, it is in doubt
/// ([`ClientError::in_doubt`]) until a resend through the session, with the same number, says what became of it. A request
/// through no session has no resend: its lost reply or its code 17 is an unknown outcome
/// ([`ClientError::outcome_unknown`]).
///
/// The core is the client's data plane: it holds no thread and no lock, and every queue it keeps is bounded by what the
/// Welcome allows, the streams and their credit, so a caller that stops reading stops the server.
pub struct ClientCore<L: Login> {
    config: ClientConfig,
    login: L,
    link: Link<L::Mechanism>,
    renewal: Option<ClientAuthentication<L::Mechanism>>,
    principal: Option<u64>,
    expires: u64,
    renew_at: Option<Instant>,
    clock: Option<(Instant, u64)>,
    sent: u128,
    generation: u64,
    next_stream: u64,
    streams: BTreeMap<u64, StreamState>,
    /// The reads cut short by a lost connection, by generation and stream, each with what its requests answered, until its
    /// handle reads that or is dropped.
    lost_reads: BTreeMap<(u64, u64), (Arc<AtomicBool>, ClientError)>,
    /// The reads whose last frame had arrived when their connection was lost, by generation and stream: read on to
    /// their end, or let go once their handle is dropped.
    kept_reads: BTreeMap<(u64, u64), (Arc<AtomicBool>, VecDeque<ReceivedFrame>)>,
    pending: BTreeMap<u64, PendingState>,
    next_pending: u64,
    sessions: BTreeMap<u128, SessionState>,
    outbox: VecDeque<Vec<u8>>,
    last_progress: Option<Instant>,
    /// Whether the last connection was lost before its handshake ended.
    handshake_lost: bool,
}

impl<L: Login> ClientCore<L> {
    /// A client configured by `config`, authenticating through `login`, not connected.
    #[must_use]
    pub fn new(config: ClientConfig, login: L) -> Self {
        Self {
            config,
            login,
            link: Link::Down,
            renewal: None,
            principal: None,
            expires: 0,
            renew_at: None,
            clock: None,
            sent: 0,
            generation: 0,
            next_stream: 1,
            streams: BTreeMap::new(),
            lost_reads: BTreeMap::new(),
            kept_reads: BTreeMap::new(),
            pending: BTreeMap::new(),
            next_pending: 1,
            sessions: BTreeMap::new(),
            outbox: VecDeque::new(),
            last_progress: None,
            handshake_lost: false,
        }
    }

    /// The configuration the client was made with.
    #[must_use]
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Starts a connection the shell has just opened at `now`, when the wall clock reads `unix_micros`, microseconds
    /// of Unix time: the Hello is queued, and frames are numbered from 1 again. The two clocks map the expiry the
    /// server states onto the core's time. Whatever the core held of a previous connection is lost with it, as
    /// [`ClientCore::disconnected`] loses it.
    pub fn connect(&mut self, now: Instant, unix_micros: u64) -> Result<(), ClientError> {
        if !matches!(self.link, Link::Down) {
            self.lose(&ClientError::connection_lost(None));
        }
        let handshake = ClientHandshake::new(self.config.cluster(), self.config.hello());
        let hello = handshake.hello_frame().map_err(ClientError::transport)?;
        self.handshake_lost = false;
        self.generation += 1;
        self.next_stream = 1;
        self.sent = 0;
        self.clock = Some((now, unix_micros));
        self.outbox.push_back(hello);
        self.link = Link::Handshake(handshake);
        self.last_progress = Some(now);
        Ok(())
    }

    /// The principal the connection acts for, once authenticated; `None` before, and for a connection to a server
    /// asking for no authentication.
    #[must_use]
    pub fn principal(&self) -> Option<u64> {
        self.principal
    }

    /// When the connection's credential expires, microseconds of Unix time, as the server's final Authenticate
    /// stated; `None` for a credential that never expires, or before one.
    #[must_use]
    pub fn expires(&self) -> Option<u64> {
        (self.expires != 0).then_some(self.expires)
    }

    /// The server's Welcome, once it arrived: through the Authenticate exchange and after it. A shell's framer
    /// reads every later frame at its `protocol`.
    #[must_use]
    pub fn welcome(&self) -> Option<&Welcome> {
        match &self.link {
            Link::Authenticating(welcome, _) | Link::Up(welcome) => Some(welcome),
            Link::Down | Link::Handshake(_) => None,
        }
    }

    /// Whether the connection is welcomed, so requests can be sent.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self.link, Link::Up(_))
    }

    /// Whether a connection is open or being opened.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        !matches!(self.link, Link::Down)
    }

    /// Whether the last connection was lost before its handshake ended: a server that accepts connections and fails
    /// their handshakes. The shell waits [`ClientConfig::retry_delay`] before connecting again, as after a connection
    /// it could not make, so the retries of a commit waiting yield and an outer timeout can end them; each such loss uses
    /// one of a waiting commit's [`ClientConfig::commit_attempts`], as a connection not made does.
    #[must_use]
    pub fn handshake_lost(&self) -> bool {
        self.handshake_lost
    }

    /// Whether the shell connects now: a commit waits to be sent again on a new connection.
    #[must_use]
    pub fn needs_connection(&self) -> bool {
        matches!(self.link, Link::Down)
            && self
                .pending
                .values()
                .any(|p| p.outcome.is_none() && p.commit().is_some())
    }

    /// The next frame to write, whole, numbered as it leaves: the connection's next sequence.
    #[must_use]
    pub fn transmit(&mut self) -> Option<Vec<u8>> {
        let mut frame = self.outbox.pop_front()?;
        self.sent = self.sent.saturating_add(1);
        // Exception (Ultra rule 7): a stamp fails only for a frame shorter than its header, and every frame the core
        // queues was built whole, header first, so no error is dropped here that can occur.
        Header::stamp_sequence(&mut frame, self.sent).ok()?;
        Some(frame)
    }

    /// The shell's connection failed or closed with `error`: every request waiting on it, and every read cut short,
    /// fails with [`ClientError::connection_lost`], its cause `error` ([`ClientError::transport`]: the framer's refusal,
    /// a frame cut short, an I/O failure), none for a plain close ([`TransportError::closed`]); a commit waits to be
    /// retried through its session on the next connection.
    pub fn disconnected(&mut self, error: TransportError, now: Instant) {
        self.last_progress = Some(now);
        let cause = (error != TransportError::closed()).then(|| ClientError::transport(error));
        self.lose(&ClientError::connection_lost(cause));
    }

    /// The shell's attempt to open a connection failed with `cause` while a commit waits to be sent again: the attempt
    /// counts as one of the commit's [`ClientConfig::commit_attempts`], as a send lost with its connection does, so an
    /// unreachable server ends the retries. A commit whose attempts are spent answers [`ClientError::in_doubt`], `cause`
    /// its cause, and its session is in doubt until [`ClientCore::resolve`]. The shell waits [`ClientConfig::retry_delay`]
    /// before its next attempt.
    pub fn connect_failed(&mut self, cause: &ClientError) {
        if !matches!(self.link, Link::Down) {
            return;
        }
        let attempts = self.config.commit_attempts();
        for pending in self
            .pending
            .values_mut()
            .filter(|p| p.outcome.is_none() && p.stream.is_none() && p.resend_at.is_none())
        {
            let Some(request) = pending.commit() else {
                continue;
            };
            pending.tries = pending.tries.saturating_add(1);
            if pending.tries >= attempts {
                let state = self.sessions.get_mut(&request.0);
                pending.mark_in_doubt(request, state, Some(cause.clone()));
            }
        }
    }

    /// Whether the core waits for the server, a read apart: a handshake, an Authenticate exchange or a request's reply.
    fn awaiting(&self) -> bool {
        matches!(self.link, Link::Handshake(_) | Link::Authenticating(..))
            || self.renewal.is_some()
            || (self.is_ready() && self.pending.values().any(PendingState::in_flight))
    }

    /// When the first read waiting on the server times out: from the later of its last progress and its transaction's
    /// deadline, since its silence within the deadline is a wait for a close, by at least [`READ_TIMEOUT_MIN`]. A read
    /// with a frame to read or no credit left waits on its caller, and runs no timeout.
    fn read_timeout(&self) -> Option<Instant> {
        let timeout = self.config.timeout().max(READ_TIMEOUT_MIN);
        self.streams
            .values()
            .filter(|s| s.results && s.inbox.is_empty() && s.credit > 0 && !s.with_caller)
            .filter_map(|s| {
                let from = [s.progress, s.deadline].into_iter().flatten().max()?;
                from.checked_add(timeout)
            })
            .min()
    }

    /// When what the core waits for times out; a timeout past what an `Instant` holds never does.
    fn wait_timeout(&self) -> Option<Instant> {
        self.last_progress
            .filter(|_| self.awaiting())
            .and_then(|at| at.checked_add(self.config.timeout()))
    }

    /// When the shell calls [`ClientCore::handle_timeout`] at the latest: the timeout of what the core waits for or of a
    /// read waiting on the server, a busy commit's next send, or the credential's renewal; `None` when nothing is awaited
    /// or due.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        let resend = self
            .pending
            .values()
            .filter(|p| p.outcome.is_none())
            .filter_map(|p| p.resend_at)
            .filter(|_| self.is_ready())
            .min();
        let renew = self
            .renew_at
            .filter(|_| self.is_ready() && self.renewal.is_none());
        [self.wait_timeout(), self.read_timeout(), resend, renew]
            .into_iter()
            .flatten()
            .min()
    }

    /// The time is `now`: a busy commit due is sent again, a credential due for renewal is renewed by a new exchange
    /// on stream 0 while the other streams go on, and a connection that answered nothing awaited within the timeout
    /// is given up, its requests failing with [`ClientError::timed_out`].
    pub fn handle_timeout(&mut self, now: Instant) {
        if self.is_ready() {
            let due: Vec<u64> = self
                .pending
                .iter()
                .filter(|(_, p)| p.outcome.is_none() && p.resend_at.is_some_and(|at| at <= now))
                .map(|(id, _)| *id)
                .collect();
            for id in due {
                self.send_commit(id, now);
            }
            if self.renewal.is_none() && self.renew_at.is_some_and(|at| at <= now) {
                self.renew(now);
            }
        }
        let timed_out = [self.wait_timeout(), self.read_timeout()]
            .into_iter()
            .flatten()
            .any(|at| at <= now);
        if timed_out {
            self.lose(&ClientError::timed_out());
        }
    }

    /// The credential renewed by a new exchange on stream 0 while the other streams go on; a login that cannot start
    /// one gives the connection up, its requests lost with the login's refusal as the cause.
    fn renew(&mut self, now: Instant) {
        let Link::Up(welcome) = &self.link else {
            return;
        };
        let welcome = welcome.clone();
        match self.exchange(&welcome) {
            Ok(exchange) => {
                self.renewal = Some(exchange);
                self.last_progress = Some(now);
            }
            Err(error) => self.lose(&ClientError::connection_lost(Some(error))),
        }
    }

    /// The credential's expiry, microseconds of Unix time, 0 for never, recorded at `now` and its renewal scheduled
    /// the configured margin before it, at most half its remaining life.
    fn schedule_renewal(&mut self, expires: u64, now: Instant) {
        self.expires = expires;
        self.renew_at = (expires != 0)
            .then(|| {
                let remaining = Duration::from_micros(expires.saturating_sub(self.unix_at(now)));
                let lead = self.config.renewal_margin().min(remaining / 2);
                // An expiry past what an `Instant` holds never falls due.
                now.checked_add(remaining - lead)
            })
            .flatten();
    }

    /// The wall clock at `now`, microseconds of Unix time, from the clocks the connection was opened at.
    fn unix_at(&self, now: Instant) -> u64 {
        self.clock.map_or(0, |(at, unix)| {
            unix.saturating_add(now.saturating_duration_since(at).as_micros() as u64)
        })
    }

    /// Takes `more` of the client's frame buffers for its reads, refused as [`ClientError::buffer_limit`] when they would
    /// not fit beside what the open reads may hold and the frames kept from a lost connection, one buffer left to read
    /// into. So credit and the buffers agree: every frame the server may send without the caller finds a buffer, and a
    /// read not yet polled never takes the one a reply needs.
    fn reserve_buffers(&mut self, more: u32) -> Result<(), ClientError> {
        self.forget_dropped_reads();
        let open: usize = self.streams.values().map(|s| s.buffers as usize).sum();
        let kept: usize = self.kept_reads.values().map(|(_, inbox)| inbox.len()).sum();
        let room = self.config.buffers().saturating_sub(1);
        if open.saturating_add(kept).saturating_add(more as usize) > room {
            return Err(ClientError::buffer_limit(self.config.buffers()));
        }
        Ok(())
    }

    /// What a lost connection kept of each read whose handle is gone, which nothing can ask for now.
    fn forget_dropped_reads(&mut self) {
        self.lost_reads.retain(|_, (handle, _)| handle_held(handle));
        self.kept_reads.retain(|_, (handle, _)| handle_held(handle));
    }

    /// The reads of the transaction of `stream` end with it.
    fn end_reads(&mut self, stream: u64) {
        self.streams.retain(|_, state| state.of != Some(stream));
    }

    /// A frame of the connection's own on stream 0, at `welcome`'s protocol, refused when larger than it allows.
    fn connection_request<B: BodyBuilder>(
        &self,
        body: &B,
        welcome: &Welcome,
    ) -> Result<Vec<u8>, ClientError> {
        let header = Header::new(self.config.cluster(), B::COMMAND)
            .with_release(self.config.release())
            .with_protocol(welcome.protocol());
        let frame = body.frame(header).map_err(ClientError::request)?;
        if frame.len() > welcome.max_frame() as usize {
            return Err(ClientError::request(ProtocolError::frame_too_large(
                frame.len(),
                welcome.max_frame() as usize,
            )));
        }
        Ok(frame)
    }

    /// Closes the connection on the client's side, and the shell shuts it down: requests waiting and reads not yet
    /// ended fail with [`ClientError::connection_lost`], no cause, a commit whose frame never left among them, and a commit
    /// whose frame left is in doubt ([`ClientError::in_doubt`]), its session resolved before it commits again.
    pub fn close(&mut self) {
        self.lose(&ClientError::connection_lost(None));
        for pending in self.pending.values_mut().filter(|p| p.outcome.is_none()) {
            if let Some(request) = pending.commit() {
                let state = self.sessions.get_mut(&request.0);
                pending.mark_in_doubt(request, state, None);
            }
        }
    }

    /// The answer of `pending`, once arrived, given once.
    #[must_use]
    pub fn take_reply(&mut self, pending: Pending) -> Option<Result<Reply, ClientError>> {
        let arrived = self
            .pending
            .get(&pending.id())
            .is_some_and(|p| p.outcome.is_some());
        if arrived {
            self.pending.remove(&pending.id())?.outcome
        } else {
            None
        }
    }

    /// A session the application kept from an earlier client, identified by `number`, its next request `next_request`.
    /// Refused as [`ClientError::request`] for a session of `number` 0, which names none, or a request of 0, which a session
    /// cannot send (`PROTOCOL.md` §6.3), and for a session this client holds with a request in flight or in doubt, which
    /// the number would overwrite. `next_request` is one above the session's last committed request (`PROTOCOL.md` §11.5),
    /// and the client cannot check it: a number below a partition's last is refused there with code 13, and the session's
    /// counter moves above that refusal's `detail`; a one-frame write or Drop numbered at the last committed request is answered
    /// with that request's stored reply (§6.6), which reads as this request's success. An application saves the number after
    /// a request before it sends that request, so a crash between the two leaves a number above the last.
    pub fn resume_session(
        &mut self,
        number: u128,
        next_request: u64,
    ) -> Result<Session, ClientError> {
        let commit = Command::Commit.number();
        if number == 0 {
            return Err(ClientError::request(ProtocolError::field_value(
                commit, "session", 0,
            )));
        }
        if next_request == 0 {
            return Err(ClientError::request(ProtocolError::field_value(
                commit, "request", 0,
            )));
        }
        if let Some(held) = self.sessions.get(&number) {
            if let Some((request, _)) = &held.in_doubt {
                return Err(ClientError::in_doubt(number, *request, None));
            }
            if held.in_flight {
                return Err(ClientError::session_busy(number));
            }
        }
        let state = self
            .sessions
            .entry(number)
            .or_insert_with(|| SessionState::new(next_request));
        state.next_request = next_request;
        Ok(Session::new(number))
    }

    /// The token of the highest epoch among `session`'s commits, one-frame writes and resolved resends; `None` for a
    /// session the client does not hold, or one no reply has given a token.
    #[must_use]
    pub fn session_token(&self, session: Session) -> Option<&SessionToken> {
        self.sessions.get(&session.number())?.token.as_ref()
    }

    /// The request number `session`'s next commit carries; `None` for a session the client does not hold.
    #[must_use]
    pub fn next_request(&self, session: Session) -> Option<u64> {
        self.sessions.get(&session.number()).map(|s| s.next_request)
    }

    /// The connection is gone: streams and queued frames with it; each request waiting fails with `reason`, a Register waiting
    /// to be sent again among them. A commit whose
    /// frame never left on any connection fails as lost; one through no session whose frame left has an unknown outcome; any
    /// other is retried on the next connection while it has attempts left, else it is in doubt, an attempt being used when
    /// its frame left on the connection lost, or when the connection was lost in its handshake. A read whose last frame
    /// had arrived is kept, to be read to its end; any other read is cut short and answers `reason`, its transaction
    /// aborted with its connection (`PROTOCOL.md` §11.4), so nothing of it is retried.
    fn lose(&mut self, reason: &ClientError) {
        self.handshake_lost = matches!(self.link, Link::Handshake(_) | Link::Authenticating(..));
        let handshake_lost = self.handshake_lost;
        self.link = Link::Down;
        self.renewal = None;
        self.principal = None;
        self.expires = 0;
        self.renew_at = None;
        let generation = self.generation;
        for (id, state) in std::mem::take(&mut self.streams) {
            if !state.results {
                continue;
            }
            let ended = state.ended();
            let Some(handle) = state.handle.filter(handle_held) else {
                continue;
            };
            let key = (generation, id);
            if ended {
                let _replaced = self.kept_reads.insert(key, (handle, state.inbox));
            } else {
                let _replaced = self.lost_reads.insert(key, (handle, reason.clone()));
            }
        }
        self.forget_dropped_reads();
        self.outbox.clear();
        let attempts = self.config.commit_attempts();
        let sent_frames = self.sent;
        for pending in self.pending.values_mut().filter(|p| p.outcome.is_none()) {
            let Some(request) = pending.commit() else {
                pending.outcome = Some(Err(reason.clone()));
                continue;
            };
            let sent = pending.stream.is_some()
                && pending
                    .sequence
                    .is_some_and(|sequence| sequence <= sent_frames);
            pending.stream = None;
            pending.sequence = None;
            pending.resend_at = None;
            pending.ever_sent |= sent;
            let state = self.sessions.get_mut(&request.0);
            if !pending.ever_sent {
                pending.outcome = Some(Err(as_lost(reason)));
                if let Some(state) = state {
                    state.in_flight = false;
                }
            } else if request.0 == 0 {
                pending.outcome = Some(Err(ClientError::outcome_unknown(request.0, request.1)));
            } else if sent || handshake_lost {
                // A send lost with its connection uses an attempt, and so does a connection lost in its handshake while
                // the commit waits for it, as one not made does: a server refusing every handshake ends the retries.
                pending.tries = pending.tries.saturating_add(1);
                if pending.tries >= attempts {
                    pending.mark_in_doubt(request, state, Some(reason.clone()));
                }
            }
        }
    }

    /// The header of a request of `B` on `stream`, and its frame, refused when larger than the Welcome allows.
    fn frame<B: BodyBuilder>(&self, body: &B, stream: u64) -> Result<Vec<u8>, ClientError> {
        let Link::Up(welcome) = &self.link else {
            return Err(ClientError::not_connected());
        };
        let header = Header::new(self.config.cluster(), B::COMMAND)
            .with_release(self.config.release())
            .with_protocol(welcome.protocol())
            .with_stream(stream);
        let frame = body.frame(header).map_err(ClientError::request)?;
        if frame.len() > welcome.max_frame() as usize {
            return Err(ClientError::request(ProtocolError::frame_too_large(
                frame.len(),
                welcome.max_frame() as usize,
            )));
        }
        Ok(frame)
    }

    /// A new stream, refused when the connection holds as many as the Welcome allows.
    fn open_stream(&mut self) -> Result<u64, ClientError> {
        let Link::Up(welcome) = &self.link else {
            return Err(ClientError::not_connected());
        };
        if self.streams.len() as u64 >= u64::from(welcome.max_streams()) {
            return Err(ClientError::stream_limit(welcome.max_streams()));
        }
        let stream = self.next_stream;
        debug_assert!(
            !self.streams.contains_key(&stream),
            "a number is never reused"
        );
        self.next_stream += 1;
        let _replaced: Option<StreamState> = self.streams.insert(stream, StreamState::default());
        Ok(stream)
    }

    /// A request with one reply: its frame queued on `stream`, waiting as `pending`.
    fn send_request(
        &mut self,
        stream: u64,
        frame: Vec<u8>,
        mut pending: PendingState,
        now: Instant,
    ) -> Pending {
        let id = self.next_pending;
        self.next_pending += 1;
        pending.sequence = Some(self.next_sequence());
        let _replaced: Option<PendingState> = self.pending.insert(id, pending);
        if let Some(state) = self.streams.get_mut(&stream) {
            state.pending = Some(id);
        }
        self.outbox.push_back(frame);
        self.last_progress = Some(now);
        Pending::new(id)
    }

    /// The frame sending request `id` again on `stream`: a Commit naming its session and request, the Drop or one-frame write
    /// as its first frame held it, or a Register.
    fn resend_frame(&self, id: u64, stream: u64) -> Result<Vec<u8>, ClientError> {
        let Some(pending) = self.pending.get(&id) else {
            return Err(ClientError::stream_finished(stream));
        };
        match (&pending.resend, pending.commit()) {
            (Resend::Drop(command), _) => self.frame(command.as_ref(), stream),
            (Resend::Direct(write), _) => self.frame(write.as_ref(), stream),
            (Resend::Conditional(write), _) => self.frame(write.as_ref(), stream),
            (Resend::Commit, Some((session, request))) => {
                self.frame(&Commit::new(session.to_be_bytes(), request, 0), stream)
            }
            (Resend::Commit, None) => self.frame(&Register::new([0; 16]), stream),
        }
    }

    /// Sends request `id` again on a stream of its own: a commit through its session with its request number, the retry the
    /// server answers from its session table if the commit was applied, or a Register answered busy.
    fn send_commit(&mut self, id: u64, now: Instant) {
        let Some(request) = self.pending.get(&id).map(PendingState::commit) else {
            return;
        };
        let opened = self.open_stream();
        let framed = opened.and_then(|stream| {
            let frame = self.resend_frame(id, stream);
            if frame.is_err() {
                let _ended: Option<StreamState> = self.streams.remove(&stream);
            }
            frame.map(|frame| (stream, frame))
        });
        let sequence = self.next_sequence();
        let Some(pending) = self.pending.get_mut(&id) else {
            return;
        };
        match framed {
            Ok((stream, frame)) => {
                pending.stream = Some(stream);
                pending.sequence = Some(sequence);
                pending.resend_at = None;
                pending.retried = true;
                if let Some(state) = self.streams.get_mut(&stream) {
                    state.pending = Some(id);
                }
                self.outbox.push_back(frame);
                self.last_progress = Some(now);
            }
            Err(cause) => {
                // A retry or a resolve: the commit was sent before and may have been applied, so it is in doubt, the
                // caller resolves it, and the session's next commit does not reuse its request number. A Register was
                // refused, so what stopped its resend is its answer.
                let Some(request) = request else {
                    pending.outcome = Some(Err(cause));
                    return;
                };
                let state = self.sessions.get_mut(&request.0);
                pending.mark_in_doubt(request, state, Some(cause));
            }
        }
    }

    /// The sequence the next frame queued leaves with: after every frame sent and every frame queued before it.
    fn next_sequence(&self) -> u128 {
        self.sent + self.outbox.len() as u128 + 1
    }

    /// Every commit waiting for a connection, sent on the new one.
    fn resend_commits(&mut self, now: Instant) {
        let waiting: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, p)| {
                p.outcome.is_none()
                    && p.stream.is_none()
                    && p.resend_at.is_none()
                    && p.commit().is_some()
            })
            .map(|(id, _)| *id)
            .collect();
        for id in waiting {
            self.send_commit(id, now);
        }
    }
}

/// `reason` as a request lost with its connection answers it: [`ClientError::connection_lost`], whatever gave it up its
/// cause.
fn as_lost(reason: &ClientError) -> ClientError {
    if matches!(reason.0, ClientErrorEnum::ConnectionLost { .. }) {
        reason.clone()
    } else {
        ClientError::connection_lost(Some(reason.clone()))
    }
}
