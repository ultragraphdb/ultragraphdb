/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_protocol_wire::{
    Abort, Begin, BodyBuilder, Commit, Describe, Execute, Operation, Prepare, Read, Register, Write,
};

use crate::errors::client_error::ClientError;
use crate::types::client_core::ClientCore;
use crate::types::pending::Pending;
use crate::types::pending_state::{Expect, PendingState};
use crate::types::result_stream::ResultStream;
use crate::types::session::Session;
use crate::types::stream_state::StreamState;
use crate::types::transaction::Transaction;

impl ClientCore {
    /// The open stream of `transaction`, free for its next request: refused as [`ClientError::StreamFinished`] when
    /// it is gone, with the refusal it ended with when it was refused, as [`ClientError::StreamBusy`] while a reply or
    /// a result is awaited on it, and as [`ClientError::ReadOnly`] for a write of a read-only transaction.
    fn transaction_stream(
        &mut self,
        transaction: &Transaction,
        writes: bool,
    ) -> Result<&mut StreamState, ClientError> {
        let stream = transaction.stream();
        if transaction.generation() != self.generation || !self.is_ready() {
            return Err(ClientError::StreamFinished(stream));
        }
        let Some(state) = self.streams.get_mut(&stream) else {
            return Err(ClientError::StreamFinished(stream));
        };
        if let Some(failed) = state.failed.take() {
            self.streams.remove(&stream);
            return Err(failed);
        }
        let Some(state) = self.streams.get_mut(&stream) else {
            return Err(ClientError::StreamFinished(stream));
        };
        if state.transaction.is_none() {
            return Err(ClientError::StreamFinished(stream));
        }
        if state.pending.is_some() || state.results {
            return Err(ClientError::StreamBusy(stream));
        }
        if writes && state.transaction == Some(true) {
            return Err(ClientError::ReadOnly(stream));
        }
        Ok(state)
    }

    /// A request of one reply on a new stream.
    fn request<B: BodyBuilder>(
        &mut self,
        body: &B,
        expect: Expect,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        if !self.is_ready() {
            return Err(ClientError::NotConnected());
        }
        let stream = self.open_stream()?;
        match self.frame(body, stream) {
            Ok(frame) => {
                Ok(self.send_request(stream, frame, PendingState::new(Some(stream), expect), now))
            }
            Err(error) => {
                self.streams.remove(&stream);
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

    /// Aborts `transaction`; no reply, its stream ends.
    pub fn abort(&mut self, transaction: Transaction) -> Result<(), ClientError> {
        self.send_on(&transaction, &Abort, false)?;
        self.streams.remove(&transaction.stream());
        Ok(())
    }

    /// A result stream of `body` on `transaction`'s stream, with the Welcome's initial credit.
    fn results<B: BodyBuilder>(
        &mut self,
        transaction: &Transaction,
        body: &B,
        now: Instant,
    ) -> Result<ResultStream, ClientError> {
        self.transaction_stream(transaction, false)?;
        let frame = self.frame(body, transaction.stream())?;
        let credit = self.welcome().map_or(0, |w| w.initial_credit());
        if let Some(state) = self.streams.get_mut(&transaction.stream()) {
            state.results = true;
            state.credit = credit;
        }
        self.outbox.push_back(frame);
        self.last_progress = Some(now);
        Ok(ResultStream::new(transaction.stream(), self.generation))
    }

    /// Reads `read`'s quad pattern in `transaction`: a result stream.
    pub fn read(
        &mut self,
        transaction: &Transaction,
        read: &Read,
        now: Instant,
    ) -> Result<ResultStream, ClientError> {
        self.results(transaction, read, now)
    }

    /// Executes a prepared statement in `transaction`: a result stream, refused as stale once its namespace's
    /// catalog changed ([`ClientError::is_stale`]).
    pub fn execute(
        &mut self,
        transaction: &Transaction,
        execute: &Execute,
        now: Instant,
    ) -> Result<ResultStream, ClientError> {
        self.results(transaction, execute, now)
    }

    /// Commits `transaction` through `session`, with the session's next request number, answered
    /// [`crate::Reply::Committed`]. Lost with its connection, it is sent again through the session, with the same
    /// session and request number, on the next connection, at most [`crate::ClientConfig::commit_attempts`] times in
    /// all; then it is in doubt ([`ClientError::RetriesExhausted`]) until [`ClientCore::resolve`].
    pub fn commit(
        &mut self,
        transaction: Transaction,
        session: Session,
        now: Instant,
    ) -> Result<Pending, ClientError> {
        let state = *self
            .sessions
            .get(&session.number())
            .ok_or(ClientError::UnknownSession(session.number()))?;
        if let Some(request) = state.in_doubt {
            return Err(ClientError::InDoubt(session.number(), request));
        }
        if state.in_flight {
            return Err(ClientError::SessionBusy(session.number()));
        }
        self.transaction_stream(&transaction, true)?;
        let request = state.next_request;
        let frame = self.frame(
            &Commit::new(session.number(), request, 0, Vec::new()),
            transaction.stream(),
        )?;
        if let Some(state) = self.sessions.get_mut(&session.number()) {
            state.in_flight = true;
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

    /// Resolves `session`'s commit in doubt: its request sent again through the session, answered from the server's
    /// record as [`crate::Reply::Committed`] if it was applied, or refused if it was not, after which its request
    /// number is the session's next again.
    pub fn resolve(&mut self, session: Session, now: Instant) -> Result<Pending, ClientError> {
        let state = self
            .sessions
            .get_mut(&session.number())
            .ok_or(ClientError::UnknownSession(session.number()))?;
        let Some(request) = state.in_doubt.take() else {
            return Err(ClientError::NotInDoubt(session.number()));
        };
        state.in_flight = true;
        let id = self.next_pending;
        self.next_pending += 1;
        let expect = Expect::Commit {
            session: session.number(),
            request,
        };
        self.pending.insert(id, PendingState::new(None, expect));
        if self.is_ready() {
            self.send_commit(id, now);
        }
        Ok(Pending::new(id))
    }
}
