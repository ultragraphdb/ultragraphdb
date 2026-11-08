/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::Login;
use ultragraphdb_protocol_wire::{
    Abort, Begin, BodyBuilder, Command, Commit, ConditionalWrite, Describe, DirectWrite,
    DropCommand, Execute, Operation, Prepare, Read, Register, Write,
};

use crate::errors::client_error::ClientError;
use crate::types::client_core::ClientCore;
use crate::types::pending::Pending;
use crate::types::pending_state::{Expect, PendingState};
use crate::types::resend::Resend;
use crate::types::result_stream::ResultStream;
use crate::types::session::Session;
use crate::types::stream_state::StreamState;
use crate::types::transaction::Transaction;

impl<L: Login> ClientCore<L> {
    /// Whether the open stream of `transaction` is free for its next request: refused as
    /// [`ClientError::stream_finished`] when it is gone, with the refusal it ended with when it was refused, as
    /// [`ClientError::stream_busy`] while a reply is awaited on it, and as [`ClientError::read_only`] for a write of a
    /// read-only transaction.
    fn transaction_stream(
        &mut self,
        transaction: &Transaction,
        writes: bool,
    ) -> Result<(), ClientError> {
        let stream = transaction.stream();
        if transaction.generation() != self.generation || !self.is_ready() {
            return Err(ClientError::stream_finished(stream));
        }
        let Some(state) = self.streams.get_mut(&stream) else {
            return Err(ClientError::stream_finished(stream));
        };
        if let Some(failed) = state.failed.take() {
            let _ended: Option<StreamState> = self.streams.remove(&stream);
            return Err(failed);
        }
        if state.transaction.is_none() {
            return Err(ClientError::stream_finished(stream));
        }
        if state.pending.is_some() {
            return Err(ClientError::stream_busy(stream));
        }
        if writes && state.transaction == Some(true) {
            return Err(ClientError::read_only(stream));
        }
        Ok(())
    }

    /// `body`'s frame on a new stream, refused with no stream left open when it cannot be built.
    fn new_stream_frame<B: BodyBuilder>(
        &mut self,
        body: &B,
    ) -> Result<(u64, Vec<u8>), ClientError> {
        if !self.is_ready() {
            return Err(ClientError::not_connected());
        }
        let stream = self.open_stream()?;
        match self.frame(body, stream) {
            Ok(frame) => Ok((stream, frame)),
            Err(error) => {
                let _ended: Option<StreamState> = self.streams.remove(&stream);
                Err(error)
            }
        }
    }

    /// A request of one reply on a new stream.
    fn request<B: BodyBuilder>(
        &mut self,
        body: &B,
        expect: Expect,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        let (stream, frame) = self.new_stream_frame(body)?;
        Ok(self.send_request(stream, frame, PendingState::new(Some(stream), expect), now))
    }

    /// The number the next commit, drop or one-frame write of `session` takes, by the session's one counter, refused while
    /// the session has a request in doubt or in flight; through no session, session and request 0.
    fn next_number(&self, session: Option<Session>) -> Result<(u128, u64), ClientError> {
        let Some(session) = session else {
            return Ok((0, 0));
        };
        let number = session.number();
        let state = self
            .sessions
            .get(&number)
            .ok_or(ClientError::unknown_session(number))?;
        if let Some((request, _)) = &state.in_doubt {
            return Err(ClientError::in_doubt(number, *request, None));
        }
        if state.in_flight {
            return Err(ClientError::session_busy(number));
        }
        Ok((number, state.next_request))
    }

    /// The token a request carries through `session`, given `own`, the one the caller set on it: the one of the higher
    /// epoch; a session whose replies gave none leaves `own` as it is.
    fn carried_token(&self, session: u128, own: Option<&SessionToken>) -> Option<SessionToken> {
        let held = self.sessions.get(&session)?.token.as_ref()?;
        (own.is_none_or(|own| held.epoch() > own.epoch())).then(|| held.clone())
    }

    /// A commit, a drop or a one-frame write on `stream`, waiting as `expect`, its session's counter taken until it is
    /// answered, with what sends it again.
    fn send_counted(
        &mut self,
        stream: u64,
        frame: Vec<u8>,
        (expect, resend): (Expect, Resend),
        now: Instant,
    ) -> Pending {
        let mut pending = PendingState::new(Some(stream), expect);
        if let Some((session, request)) = pending.commit() {
            debug_assert_eq!(
                session == 0,
                request == 0,
                "a number through a session alone"
            );
        }
        if let Some((session, _)) = pending.commit()
            && let Some(state) = self.sessions.get_mut(&session)
        {
            state.in_flight = true;
        }
        pending.resend = resend;
        self.send_request(stream, frame, pending, now)
    }

    /// Registers a session (D9), answered [`crate::Reply::Registered`]. A refusal with code 14, a full registry with no
    /// session it may evict, sends the Register again after [`crate::ClientConfig::retry_delay`], the attempts
    /// [`crate::ClientConfig::commit_attempts`], and the last such refusal is then its answer.
    pub fn register(&mut self, now: Instant) -> Result<Pending, ClientError> {
        self.request(&Register::new([0; 16]), Expect::Register, now)
    }

    /// Begins a transaction in `tenant` on a new stream, at the default level and deadline and carrying no token, answered
    /// [`crate::Reply::Begun`] with its snapshot.
    pub fn begin(
        &mut self,
        tenant: u64,
        read_only: bool,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        self.request(&Begin::new(tenant, 0, read_only), Expect::Begin, now)
    }

    /// Begins the transaction `begin` names, its level, deadline, read version and tenant, on a new stream, answered
    /// [`crate::Reply::Begun`]; through a session it carries the session's token, or `begin`'s own when that is of a higher
    /// epoch.
    ///
    /// # Errors
    ///
    /// [`ClientError::unknown_session`] for a session this client does not hold; [`ClientError::not_connected`] before
    /// the Welcome; [`ClientError::stream_limit`] while the connection holds as many streams as the Welcome allows;
    /// [`ClientError::request`] for a frame that cannot be built or is larger than the largest frame.
    pub fn begin_with(
        &mut self,
        begin: Begin,
        session: impl Into<Option<Session>>,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        let session: Option<Session> = session.into();
        let begin = match session {
            Some(session) if !self.sessions.contains_key(&session.number()) => {
                return Err(ClientError::unknown_session(session.number()));
            }
            Some(session) => match self.carried_token(session.number(), begin.token()) {
                Some(token) => begin.with_token(token),
                None => begin,
            },
            None => begin,
        };
        self.request(&begin, Expect::Begin, now)
    }

    /// Prepares `prepare`'s statement, answered [`crate::Reply::Prepared`].
    pub fn prepare(&mut self, prepare: &Prepare, now: Instant) -> Result<Pending, ClientError> {
        self.request(prepare, Expect::Prepare, now)
    }

    /// Asks for `tenant`'s data dictionary, answered [`crate::Reply::Described`].
    pub fn describe(&mut self, tenant: u64, now: Instant) -> Result<Pending, ClientError> {
        self.request(&Describe::request(tenant), Expect::Describe, now)
    }

    /// Queues a frame on a transaction's stream, a request without a reply.
    fn send_on<B: BodyBuilder>(
        &mut self,
        transaction: &Transaction,
        body: &B,
        writes: bool,
    ) -> Result<(), ClientError> {
        self.transaction_stream(transaction, writes)?;
        let frame = self.frame(body, transaction.stream())?;
        self.outbox.push_back(frame);
        Ok(())
    }

    /// Writes `write`'s changeset in `transaction`; no reply, a refusal ends the transaction.
    pub fn write(&mut self, transaction: &Transaction, write: &Write) -> Result<(), ClientError> {
        self.send_on(transaction, write, true)
    }

    /// Asks for a commit-time operation in `transaction`; its result comes with the commit's answer.
    pub fn operation(
        &mut self,
        transaction: &Transaction,
        operation: &Operation,
    ) -> Result<(), ClientError> {
        self.send_on(transaction, operation, true)
    }

    /// Aborts `transaction`; no reply, its stream ends, and its reads end with it.
    pub fn abort(&mut self, transaction: Transaction) -> Result<(), ClientError> {
        self.send_on(&transaction, &Abort, false)?;
        let _ended: Option<StreamState> = self.streams.remove(&transaction.stream());
        self.end_reads(transaction.stream());
        Ok(())
    }

    /// A result stream of `body`, naming the transaction of stream `named`, on a stream of its own beside
    /// `transaction`'s, with the Welcome's initial credit, holding as many of the client's frame buffers as the larger of
    /// that and its own credit, and a refusal, can fill; refused as [`ClientError::buffer_limit`] before it is sent when
    /// they do not fit.
    fn results<B: BodyBuilder>(
        &mut self,
        transaction: &Transaction,
        body: &B,
        named: u64,
        now: Instant,
    ) -> Result<ResultStream, ClientError> {
        if named != transaction.stream() {
            return Err(ClientError::transaction_mismatch(
                transaction.stream(),
                named,
            ));
        }
        self.transaction_stream(transaction, false)?;
        let credit = self.welcome().map_or(0, |w| w.initial_credit());
        let buffers = credit.max(self.config.credit()).saturating_add(1);
        self.reserve_buffers(buffers)?;
        let stream = self.open_stream()?;
        let frame = match self.frame(body, stream) {
            Ok(frame) => frame,
            Err(error) => {
                let _ended: Option<StreamState> = self.streams.remove(&stream);
                return Err(error);
            }
        };
        let deadline = self
            .streams
            .get(&transaction.stream())
            .and_then(|state| state.deadline);
        let handle = ResultStream::new(stream, self.generation);
        if let Some(state) = self.streams.get_mut(&stream) {
            state.results = true;
            state.credit = credit;
            state.buffers = buffers;
            state.of = Some(transaction.stream());
            state.progress = Some(now);
            state.deadline = deadline;
            state.handle = Some(handle.tracker());
        }
        self.outbox.push_back(frame);
        self.last_progress = Some(now);
        Ok(handle)
    }

    /// Reads `read`'s quad pattern in `transaction`: a result stream of its own, naming the transaction, with the
    /// Welcome's initial credit; refused as [`ClientError::transaction_mismatch`] when `read` names another, and as
    /// [`ClientError::buffer_limit`] while the client's frame buffers hold what its open reads may bring and this one's
    /// would not fit: one of them is read to its end or cancelled first.
    pub fn read(
        &mut self,
        transaction: &Transaction,
        read: &Read,
        now: Instant,
    ) -> Result<ResultStream, ClientError> {
        self.results(transaction, read, read.transaction(), now)
    }

    /// Executes a prepared statement in `transaction`: a result stream of its own, as [`ClientCore::read`]'s,
    /// refused as stale once its tenant's catalog changed ([`ClientError::is_stale`]).
    pub fn execute(
        &mut self,
        transaction: &Transaction,
        execute: &Execute,
        now: Instant,
    ) -> Result<ResultStream, ClientError> {
        self.results(transaction, execute, execute.transaction(), now)
    }

    /// Commits `transaction` through `session`, with the session's next request number, answered
    /// [`crate::Reply::Committed`]. Lost with its connection, it is sent again through the session, with the same
    /// session and request number, on the next connection, at most [`crate::ClientConfig::commit_attempts`] times in
    /// all; then it is in doubt ([`ClientError::in_doubt`]) until [`ClientCore::resolve`]. Through no session, the path a
    /// transaction of declarations takes (`PROTOCOL.md` §11.13), it has no resend, and a lost reply or a code 17 is an unknown
    /// outcome. It ends the reads still open: read one to its `LAST` first when its every row is wanted.
    pub fn commit(
        &mut self,
        transaction: Transaction,
        session: impl Into<Option<Session>>,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        let (session, request) = self.next_number(session.into())?;
        self.transaction_stream(&transaction, true)?;
        let stream = transaction.stream();
        let frame = self.frame(&Commit::new(session.to_be_bytes(), request, 0), stream)?;
        let expect = Expect::Commit { session, request };
        Ok(self.send_counted(stream, frame, (expect, Resend::Commit), now))
    }

    /// Drops what `command` names through `session`, with the session's next request number, on a stream of its own,
    /// answered [`crate::Reply::Dropped`] with the drop's commit version. Lost with its connection, it is sent again
    /// through the session as a commit is, with the same session and request number, so the server applies it once.
    pub fn drop_command(
        &mut self,
        command: DropCommand,
        session: impl Into<Option<Session>>,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        let (session, request) = self.next_number(session.into())?;
        let command = command.with_session(session.to_be_bytes(), request);
        let (stream, frame) = self.new_stream_frame(&command)?;
        let expect = Expect::Drop { session, request };
        Ok(self.send_counted(
            stream,
            frame,
            (expect, Resend::Drop(Box::new(command))),
            now,
        ))
    }

    /// Writes `write` in one frame, unconditional changes committed as one transaction on a stream of their own, no
    /// Begin and no open transaction, answered [`crate::Reply::Committed`]; through `session` it takes the session's next
    /// request number and carries its token, under the same rules as a commit. A write computed from an earlier read is a
    /// [`ClientCore::conditional_write`] or a read-write transaction: sent as a read and then this, a commit between them
    /// is a lost update the server cannot see (`PROTOCOL.md` §11.2a).
    ///
    /// # Errors
    ///
    /// [`ClientError::request`] for a `principal` other than 0; [`ClientError::unknown_session`] for a session this
    /// client does not hold, [`ClientError::in_doubt`] for one with a request in doubt, [`ClientError::session_busy`]
    /// for one with a request in flight; [`ClientError::not_connected`] before the Welcome;
    /// [`ClientError::stream_limit`] while the connection holds as many streams as the Welcome allows;
    /// [`ClientError::request`] for a frame that cannot be built or is larger than the largest frame.
    pub fn direct_write(
        &mut self,
        write: DirectWrite,
        session: impl Into<Option<Session>>,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        if write.principal() != 0 {
            return Err(ClientError::request(ProtocolError::field_value(
                Command::DirectWrite.number(),
                "principal",
                write.principal(),
            )));
        }
        let (session, request) = self.next_number(session.into())?;
        let token = self.carried_token(session, write.token());
        let write = write.with_session(session.to_be_bytes(), request);
        let write = match token {
            Some(token) => write.with_token(token),
            None => write,
        };
        let (stream, frame) = self.new_stream_frame(&write)?;
        let expect = Expect::Write { session, request };
        let resend = Resend::Direct(Box::new(write));
        Ok(self.send_counted(stream, frame, (expect, resend), now))
    }

    /// Writes `write` in one frame, its commit-time operations decided in commit order and their outcome in the reply, as
    /// [`ClientCore::direct_write`] does: a failed condition is an answer, refused with code 8 and the value found, and is
    /// not retried.
    ///
    /// # Errors
    ///
    /// As [`ClientCore::direct_write`] refuses: [`ClientError::request`] for a `principal` other than 0;
    /// [`ClientError::unknown_session`] for a session this client does not hold, [`ClientError::in_doubt`] for one
    /// with a request in doubt, [`ClientError::session_busy`] for one with a request in flight;
    /// [`ClientError::not_connected`] before the Welcome; [`ClientError::stream_limit`] while the connection holds as
    /// many streams as the Welcome allows; [`ClientError::request`] for a frame that cannot be built or is larger than
    /// the largest frame.
    pub fn conditional_write(
        &mut self,
        write: ConditionalWrite,
        session: impl Into<Option<Session>>,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        if write.principal() != 0 {
            return Err(ClientError::request(ProtocolError::field_value(
                Command::ConditionalWrite.number(),
                "principal",
                write.principal(),
            )));
        }
        let (session, request) = self.next_number(session.into())?;
        let token = self.carried_token(session, write.token());
        let write = write.with_session(session.to_be_bytes(), request);
        let write = match token {
            Some(token) => write.with_token(token),
            None => write,
        };
        let (stream, frame) = self.new_stream_frame(&write)?;
        let expect = Expect::Write { session, request };
        let resend = Resend::Conditional(Box::new(write));
        Ok(self.send_counted(stream, frame, (expect, resend), now))
    }

    /// Resolves `session`'s request in doubt by sending it again with its number, as `PROTOCOL.md` §11.5 has it: a Commit on
    /// a stream of its own with no transaction, a Drop or a one-frame write as its first frame held it. Its reply says it
    /// committed, and moves the counter; a Commit's code 18, or a Drop's or a one-frame write's refusal other than codes 12,
    /// 13, 14 and 17, says it did not, and its number is the session's next again; codes 14 and 17 send it again after the
    /// retry delay, and leave it in doubt once its attempts are spent; codes 12 and 13 leave its outcome unknown for good.
    pub fn resolve(&mut self, session: Session, now: Instant) -> Result<Pending, ClientError> {
        let number = session.number();
        let state = self
            .sessions
            .get_mut(&number)
            .ok_or(ClientError::unknown_session(number))?;
        let Some((request, resend)) = state.in_doubt.take() else {
            return Err(ClientError::not_in_doubt(number));
        };
        state.in_flight = true;
        let id = self.next_pending;
        self.next_pending += 1;
        let expect = match &resend {
            Resend::Commit => Expect::Commit {
                session: number,
                request,
            },
            Resend::Drop(_) => Expect::Drop {
                session: number,
                request,
            },
            Resend::Direct(_) | Resend::Conditional(_) => Expect::Write {
                session: number,
                request,
            },
        };
        let mut pending = PendingState::new(None, expect);
        pending.ever_sent = true;
        pending.resend = resend;
        let _replaced: Option<PendingState> = self.pending.insert(id, pending);

        if self.is_ready() {
            self.send_commit(id, now);
        }
        Ok(Pending::new(id))
    }
}
