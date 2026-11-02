/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_protocol_transport::Login;
use ultragraphdb_protocol_wire::{
    Abort, Begin, BodyBuilder, Commit, Describe, DropCommand, Execute, Operation, Prepare, Read,
    Register, Write,
};

use crate::errors::client_error::ClientError;
use crate::types::client_core::ClientCore;
use crate::types::pending::Pending;
use crate::types::pending_state::{Expect, PendingState};
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

    /// A request of one reply on a new stream.
    fn request<B: BodyBuilder>(
        &mut self,
        body: &B,
        expect: Expect,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        if !self.is_ready() {
            return Err(ClientError::not_connected());
        }
        let stream = self.open_stream()?;
        match self.frame(body, stream) {
            Ok(frame) => {
                Ok(self.send_request(stream, frame, PendingState::new(Some(stream), expect), now))
            }
            Err(error) => {
                let _ended: Option<StreamState> = self.streams.remove(&stream);
                Err(error)
            }
        }
    }

    /// Registers a session (D9), answered [`crate::Reply::Registered`].
    pub fn register(&mut self, now: Instant) -> Result<Pending, ClientError> {
        self.request(&Register::new(0), Expect::Register, now)
    }

    /// Begins a transaction in `namespace` on a new stream, answered [`crate::Reply::Begun`] with its snapshot.
    pub fn begin(
        &mut self,
        namespace: u64,
        read_only: bool,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        self.request(&Begin::new(namespace, 0, read_only), Expect::Begin, now)
    }

    /// Prepares `prepare`'s statement, answered [`crate::Reply::Prepared`].
    pub fn prepare(&mut self, prepare: &Prepare, now: Instant) -> Result<Pending, ClientError> {
        self.request(prepare, Expect::Prepare, now)
    }

    /// Asks for `namespace`'s data dictionary, answered [`crate::Reply::Described`].
    pub fn describe(&mut self, namespace: u64, now: Instant) -> Result<Pending, ClientError> {
        self.request(&Describe::request(namespace), Expect::Describe, now)
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
    /// `transaction`'s, with the Welcome's initial credit.
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
        let stream = self.open_stream()?;
        let frame = match self.frame(body, stream) {
            Ok(frame) => frame,
            Err(error) => {
                let _ended: Option<StreamState> = self.streams.remove(&stream);
                return Err(error);
            }
        };
        let credit = self.welcome().map_or(0, |w| w.initial_credit());
        if let Some(state) = self.streams.get_mut(&stream) {
            state.results = true;
            state.credit = credit;
            state.of = Some(transaction.stream());
        }
        self.outbox.push_back(frame);
        self.last_progress = Some(now);
        Ok(ResultStream::new(stream, self.generation))
    }

    /// Reads `read`'s quad pattern in `transaction`: a result stream of its own, naming the transaction, with the
    /// Welcome's initial credit; refused as [`ClientError::transaction_mismatch`] when `read` names another.
    pub fn read(
        &mut self,
        transaction: &Transaction,
        read: &Read,
        now: Instant,
    ) -> Result<ResultStream, ClientError> {
        self.results(transaction, read, read.transaction(), now)
    }

    /// Executes a prepared statement in `transaction`: a result stream of its own, as [`ClientCore::read`]'s,
    /// refused as stale once its namespace's catalog changed ([`ClientError::is_stale`]).
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
    /// all; then it is in doubt ([`ClientError::retries_exhausted`]) until [`ClientCore::resolve`].
    pub fn commit(
        &mut self,
        transaction: Transaction,
        session: Session,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        let state = *self
            .sessions
            .get(&session.number())
            .ok_or(ClientError::unknown_session(session.number()))?;
        if let Some(request) = state.in_doubt {
            return Err(ClientError::in_doubt(session.number(), request));
        }
        if state.in_flight {
            return Err(ClientError::session_busy(session.number()));
        }
        self.transaction_stream(&transaction, true)?;
        let request = state.next_request;
        let frame = self.frame(
            &Commit::new(session.number(), request, 0, Vec::new()),
            transaction.stream(),
        )?;
        if let Some(session_state) = self.sessions.get_mut(&session.number()) {
            session_state.in_flight = true;
        }
        let expect = Expect::Commit {
            session: session.number(),
            request,
        };
        Ok(self.send_request(
            transaction.stream(),
            frame,
            PendingState::new(Some(transaction.stream()), expect),
            now,
        ))
    }

    /// Drops what `command` names through `session`, with the session's next request number, on a stream of its own,
    /// answered [`crate::Reply::Dropped`] with the drop's commit version. Lost with its connection, it is sent again
    /// through the session as a commit is, with the same session and request number, so the server applies it once.
    pub fn drop_command(
        &mut self,
        command: DropCommand,
        session: Session,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        let state = *self
            .sessions
            .get(&session.number())
            .ok_or(ClientError::unknown_session(session.number()))?;
        if let Some(request) = state.in_doubt {
            return Err(ClientError::in_doubt(session.number(), request));
        }
        if state.in_flight {
            return Err(ClientError::session_busy(session.number()));
        }
        if !self.is_ready() {
            return Err(ClientError::not_connected());
        }
        let request = state.next_request;
        let command = command.with_session(session.number(), request);
        let stream = self.open_stream()?;
        let frame = match self.frame(&command, stream) {
            Ok(frame) => frame,
            Err(error) => {
                let _ended: Option<StreamState> = self.streams.remove(&stream);
                return Err(error);
            }
        };
        if let Some(session_state) = self.sessions.get_mut(&session.number()) {
            session_state.in_flight = true;
        }
        let mut pending = PendingState::new(
            Some(stream),
            Expect::Drop {
                session: session.number(),
                request,
            },
        );
        pending.drop = Some(command);
        Ok(self.send_request(stream, frame, pending, now))
    }

    /// Resolves `session`'s commit in doubt: its request sent again through the session, answered from the server's
    /// record as [`crate::Reply::Committed`] if it was applied, or refused if it was not, after which its request
    /// number is the session's next again.
    pub fn resolve(&mut self, session: Session, now: Instant) -> Result<Pending, ClientError> {
        let state = self
            .sessions
            .get_mut(&session.number())
            .ok_or(ClientError::unknown_session(session.number()))?;
        let Some(request) = state.in_doubt.take() else {
            return Err(ClientError::not_in_doubt(session.number()));
        };
        state.in_flight = true;
        let id = self.next_pending;
        self.next_pending += 1;
        let expect = Expect::Commit {
            session: session.number(),
            request,
        };
        let _replaced: Option<PendingState> =
            self.pending.insert(id, PendingState::new(None, expect));

        if self.is_ready() {
            self.send_commit(id, now);
        }
        Ok(Pending::new(id))
    }
}
