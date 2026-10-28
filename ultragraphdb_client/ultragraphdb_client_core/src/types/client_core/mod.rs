/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod client_core_receive;
mod client_core_requests;
mod client_core_results;

use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use ultragraphdb_protocol_transport::{ClientHandshake, TransportError};
use ultragraphdb_protocol_wire::{BodyBuilder, Commit, Header, Welcome};

use crate::errors::client_error::ClientError;
use crate::types::client_config::ClientConfig;
use crate::types::link::Link;
use crate::types::pending::Pending;
use crate::types::pending_state::PendingState;
use crate::types::reply::Reply;
use crate::types::session::Session;
use crate::types::session_state::SessionState;
use crate::types::stream_state::StreamState;

/// The client, sans-I/O: every rule of the protocol a client follows, and no I/O and no clock.
///
/// A shell connects, then calls [`ClientCore::connect`], writes every frame [`ClientCore::transmit`] gives, hands
/// every frame it reads, whole, to [`ClientCore::receive`] and a failed connection to
/// [`ClientCore::disconnected`], sleeps no longer than [`ClientCore::next_deadline`] and then calls
/// [`ClientCore::handle_timeout`], and connects again whenever [`ClientCore::needs_connection`] says a commit waits
/// to be retried. Requests start with a call naming the time; a request with one reply answers through
/// [`ClientCore::take_reply`], a result stream through [`ClientCore::next_batch`].
///
/// Streams are numbered from 1 on each connection and never reused on it. A transaction is a stream: its Begin, its
/// reads, writes and operations, and its Commit or Abort go on it. Write, Operation and Abort have no reply; a refusal
/// of one ends the transaction's stream and is the answer of its next request.
#[derive(Debug)]
pub struct ClientCore {
    config: ClientConfig,
    link: Link,
    generation: u64,
    next_stream: u64,
    streams: BTreeMap<u64, StreamState>,
    pending: BTreeMap<u64, PendingState>,
    next_pending: u64,
    sessions: BTreeMap<u64, SessionState>,
    outbox: VecDeque<Vec<u8>>,
    last_progress: Option<Instant>,
}

impl ClientCore {
    /// A client configured by `config`, not connected.
    pub fn new(config: ClientConfig) -> Self {
        Self {
            config,
            link: Link::Down,
            generation: 0,
            next_stream: 1,
            streams: BTreeMap::new(),
            pending: BTreeMap::new(),
            next_pending: 1,
            sessions: BTreeMap::new(),
            outbox: VecDeque::new(),
            last_progress: None,
        }
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Starts a connection the shell has just opened: the Hello is queued. Whatever the core held of a previous
    /// connection is lost with it, as [`ClientCore::disconnected`] loses it.
    pub fn connect(&mut self, now: Instant) -> Result<(), ClientError> {
        if self.link != Link::Down {
            self.lose(ClientError::ConnectionLost());
        }
        let handshake = ClientHandshake::new(self.config.cluster(), self.config.hello());
        let hello = handshake.hello_frame().map_err(ClientError::Transport)?;
        self.generation += 1;
        self.next_stream = 1;
        self.outbox.push_back(hello);
        self.link = Link::Handshake(handshake);
        self.last_progress = Some(now);
        Ok(())
    }

    /// The server's Welcome, once the handshake is done.
    pub fn welcome(&self) -> Option<&Welcome> {
        match &self.link {
            Link::Up(welcome) => Some(welcome),
            _ => None,
        }
    }

    /// Whether the connection is welcomed, so requests can be sent.
    pub fn is_ready(&self) -> bool {
        matches!(self.link, Link::Up(_))
    }

    /// Whether a connection is open or being opened.
    pub fn is_connected(&self) -> bool {
        self.link != Link::Down
    }

    /// Whether the shell connects now: a commit waits to be sent again on a new connection.
    pub fn needs_connection(&self) -> bool {
        self.link == Link::Down
            && self
                .pending
                .values()
                .any(|p| p.outcome.is_none() && p.commit().is_some())
    }

    /// The next frame to write, whole.
    pub fn transmit(&mut self) -> Option<Vec<u8>> {
        self.outbox.pop_front()
    }

    /// The shell's connection failed or closed with `error`: every request waiting on it fails with
    /// [`ClientError::Transport`], and a commit waits to be retried through its session on the next connection.
    pub fn disconnected(&mut self, error: TransportError, now: Instant) {
        self.last_progress = Some(now);
        self.lose(ClientError::Transport(error));
    }

    /// Whether the core waits for the server: a handshake, a request's reply, or a result's next frame.
    fn awaiting(&self) -> bool {
        matches!(self.link, Link::Handshake(_))
            || (self.is_ready()
                && (self.pending.values().any(PendingState::in_flight)
                    || self
                        .streams
                        .values()
                        .any(|s| s.results && s.inbox.is_empty())))
    }

    /// When the shell calls [`ClientCore::handle_timeout`] at the latest: the timeout of what the core waits for,
    /// or a busy commit's next send; `None` when nothing is awaited.
    pub fn next_deadline(&self) -> Option<Instant> {
        let timeout = self
            .last_progress
            .filter(|_| self.awaiting())
            .map(|at| at + self.config.timeout());
        let resend = self
            .pending
            .values()
            .filter(|p| p.outcome.is_none())
            .filter_map(|p| p.resend_at)
            .filter(|_| self.is_ready())
            .min();
        match (timeout, resend) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// The time is `now`: a busy commit due is sent again, and a connection that answered nothing awaited within the
    /// timeout is given up, its requests failing with [`ClientError::TimedOut`].
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
        }
        if self.awaiting()
            && self
                .last_progress
                .is_some_and(|at| now >= at + self.config.timeout())
        {
            self.lose(ClientError::TimedOut());
        }
    }

    /// Closes the connection on the client's side, and the shell shuts it down: requests waiting fail with
    /// [`ClientError::ConnectionLost`], and a commit not yet answered is in doubt ([`ClientError::InDoubt`]), its
    /// session resolved before it commits again.
    pub fn close(&mut self) {
        self.lose(ClientError::ConnectionLost());
        for pending in self.pending.values_mut().filter(|p| p.outcome.is_none()) {
            if let Some((session, request)) = pending.commit() {
                pending.outcome = Some(Err(ClientError::InDoubt(session, request)));
                if let Some(state) = self.sessions.get_mut(&session) {
                    state.in_flight = false;
                    state.in_doubt = Some(request);
                }
            }
        }
    }

    /// The answer of `pending`, once arrived, given once.
    pub fn take_reply(&mut self, pending: Pending) -> Option<Result<Reply, ClientError>> {
        self.pending.get(&pending.id())?.outcome.as_ref()?;
        self.pending.remove(&pending.id())?.outcome
    }

    /// A session the application kept from an earlier client, numbered `number`, its next commit `next_request`.
    pub fn resume_session(&mut self, number: u64, next_request: u64) -> Session {
        self.sessions.insert(
            number,
            SessionState {
                next_request,
                in_flight: false,
                in_doubt: None,
            },
        );
        Session::new(number)
    }

    /// The request number `session`'s next commit carries; `None` for a session the client does not hold.
    pub fn next_request(&self, session: Session) -> Option<u64> {
        self.sessions.get(&session.number()).map(|s| s.next_request)
    }

    /// The connection is gone: streams and queued frames with it; each request waiting fails with `reason`, and
    /// each commit is retried on the next connection while it has attempts left, else it is in doubt.
    fn lose(&mut self, reason: ClientError) {
        self.link = Link::Down;
        self.streams.clear();
        self.outbox.clear();
        let attempts = self.config.commit_attempts();
        for pending in self.pending.values_mut().filter(|p| p.outcome.is_none()) {
            match pending.commit() {
                None => {
                    if pending.stream.is_some() {
                        pending.outcome = Some(Err(reason.clone()));
                    }
                }
                Some((session, request)) => {
                    pending.tries += 1;
                    pending.stream = None;
                    pending.resend_at = None;
                    if pending.tries >= attempts {
                        pending.outcome = Some(Err(ClientError::RetriesExhausted(
                            session,
                            request,
                            pending.tries,
                        )));
                        if let Some(state) = self.sessions.get_mut(&session) {
                            state.in_flight = false;
                            state.in_doubt = Some(request);
                        }
                    }
                }
            }
        }
    }

    /// The header of a request of `B` on `stream`, and its frame, refused when larger than the Welcome allows.
    fn frame<B: BodyBuilder>(&self, body: &B, stream: u64) -> Result<Vec<u8>, ClientError> {
        let welcome = self.welcome().ok_or(ClientError::NotConnected())?;
        let header = Header::new(self.config.cluster(), B::COMMAND)
            .with_release(self.config.release())
            .with_protocol(welcome.protocol())
            .with_stream(stream);
        let frame = body.frame(header).map_err(ClientError::Request)?;
        if frame.len() > welcome.max_frame() as usize {
            return Err(ClientError::Request(
                ultragraphdb_error_protocol::ProtocolError::FrameTooLarge(
                    frame.len(),
                    welcome.max_frame() as usize,
                ),
            ));
        }
        Ok(frame)
    }

    /// A new stream, refused when the connection holds as many as the Welcome allows.
    fn open_stream(&mut self) -> Result<u64, ClientError> {
        let welcome = self.welcome().ok_or(ClientError::NotConnected())?;
        if self.streams.len() as u64 >= u64::from(welcome.max_streams()) {
            return Err(ClientError::StreamLimit(welcome.max_streams()));
        }
        let stream = self.next_stream;
        self.next_stream += 1;
        self.streams.insert(stream, StreamState::default());
        Ok(stream)
    }

    /// A request with one reply: its frame queued on `stream`, waiting as `pending`.
    fn send_request(
        &mut self,
        stream: u64,
        frame: Vec<u8>,
        pending: PendingState,
        now: Instant,
    ) -> Pending {
        let id = self.next_pending;
        self.next_pending += 1;
        self.pending.insert(id, pending);
        if let Some(state) = self.streams.get_mut(&stream) {
            state.pending = Some(id);
        }
        self.outbox.push_back(frame);
        self.last_progress = Some(now);
        Pending::new(id)
    }

    /// Sends commit `id` again, through its session with its request number, on a stream of its own: the retry the
    /// server answers from its session table if the commit was applied.
    fn send_commit(&mut self, id: u64, now: Instant) {
        let Some((session, request)) = self.pending.get(&id).and_then(PendingState::commit) else {
            return;
        };
        let sent = self.open_stream().and_then(|stream| {
            self.frame(&Commit::new(session, request, 0, Vec::new()), stream)
                .map(|frame| (stream, frame))
        });
        let Some(pending) = self.pending.get_mut(&id) else {
            return;
        };
        match sent {
            Ok((stream, frame)) => {
                pending.stream = Some(stream);
                pending.resend_at = None;
                if let Some(state) = self.streams.get_mut(&stream) {
                    state.pending = Some(id);
                }
                self.outbox.push_back(frame);
                self.last_progress = Some(now);
            }
            Err(error) => {
                pending.outcome = Some(Err(error));
                if let Some(state) = self.sessions.get_mut(&session) {
                    state.in_flight = false;
                }
            }
        }
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
