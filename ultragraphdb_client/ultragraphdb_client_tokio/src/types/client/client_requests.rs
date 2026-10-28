/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_client_core::{
    BatchPoll, ClientError, Committed, Reply, ResultStream, Session, Statement, Transaction,
};
use ultragraphdb_protocol_wire::{Describe, Execute, Operation, Prepare, Read, ResultView, Write};

use crate::traits::connect::Connect;
use crate::types::client::{Client, now};

/// A reply of another kind than its request's: the core answers each request with its own kind.
fn mismatch() -> ClientError {
    ClientError::ReplyMismatch("reply", 0, 0)
}

impl<C: Connect> Client<C> {
    /// Registers a session (D9), through which commits are sent and retried.
    pub async fn register(&mut self) -> Result<Session, ClientError> {
        self.ready().await?;
        let pending = self.core.register(now())?;
        match self.settle(pending).await? {
            Reply::Registered(session) => Ok(session),
            _ => Err(mismatch()),
        }
    }

    /// A session kept from an earlier client, its next commit numbered `next_request`.
    pub fn resume_session(&mut self, number: u64, next_request: u64) -> Session {
        self.core.resume_session(number, next_request)
    }

    /// Begins a transaction in `namespace`, read-only or read-write.
    pub async fn begin(
        &mut self,
        namespace: u64,
        read_only: bool,
    ) -> Result<Transaction, ClientError> {
        self.ready().await?;
        let pending = self.core.begin(namespace, read_only, now())?;
        match self.settle(pending).await? {
            Reply::Begun(transaction) => Ok(transaction),
            _ => Err(mismatch()),
        }
    }

    /// Writes a changeset in `transaction`; a refusal is the answer of its next request.
    pub async fn write(
        &mut self,
        transaction: &Transaction,
        write: &Write,
    ) -> Result<(), ClientError> {
        self.core.write(transaction, write)?;
        self.flush().await;
        Ok(())
    }

    /// Asks for a commit-time operation in `transaction`; its result comes with the commit.
    pub async fn operation(
        &mut self,
        transaction: &Transaction,
        operation: &Operation,
    ) -> Result<(), ClientError> {
        self.core.operation(transaction, operation)?;
        self.flush().await;
        Ok(())
    }

    /// Aborts `transaction`.
    pub async fn abort(&mut self, transaction: Transaction) -> Result<(), ClientError> {
        self.core.abort(transaction)?;
        self.flush().await;
        Ok(())
    }

    /// Commits `transaction` through `session`, retried only through the session, with the same session and request
    /// number, across reconnects, so the server applies it once.
    pub async fn commit(
        &mut self,
        transaction: Transaction,
        session: Session,
    ) -> Result<Committed, ClientError> {
        let pending = self.core.commit(transaction, session, now())?;
        match self.settle(pending).await? {
            Reply::Committed(committed) => Ok(committed),
            _ => Err(mismatch()),
        }
    }

    /// Resolves `session`'s commit in doubt: committed from the server's record, or refused when it was not applied.
    pub async fn resolve(&mut self, session: Session) -> Result<Committed, ClientError> {
        self.ready().await?;
        let pending = self.core.resolve(session, now())?;
        match self.settle(pending).await? {
            Reply::Committed(committed) => Ok(committed),
            _ => Err(mismatch()),
        }
    }

    /// Prepares a statement.
    pub async fn prepare(&mut self, prepare: &Prepare) -> Result<Statement, ClientError> {
        self.ready().await?;
        let pending = self.core.prepare(prepare, now())?;
        match self.settle(pending).await? {
            Reply::Prepared(statement) => Ok(statement),
            _ => Err(mismatch()),
        }
    }

    /// `namespace`'s data dictionary.
    pub async fn describe(&mut self, namespace: u64) -> Result<Describe, ClientError> {
        self.ready().await?;
        let pending = self.core.describe(namespace, now())?;
        match self.settle(pending).await? {
            Reply::Described(describe) => Ok(describe),
            _ => Err(mismatch()),
        }
    }

    /// Reads a quad pattern in `transaction`: a result stream.
    pub async fn read(
        &mut self,
        transaction: &Transaction,
        read: &Read,
    ) -> Result<ResultStream, ClientError> {
        let stream = self.core.read(transaction, read, now())?;
        self.flush().await;
        Ok(stream)
    }

    /// Executes a prepared statement in `transaction`: a result stream.
    pub async fn execute(
        &mut self,
        transaction: &Transaction,
        execute: &Execute,
    ) -> Result<ResultStream, ClientError> {
        let stream = self.core.execute(transaction, execute, now())?;
        self.flush().await;
        Ok(stream)
    }

    /// The next batch of `stream`, read in place by `read`; `None` after the last.
    pub async fn next_batch<T>(
        &mut self,
        stream: &ResultStream,
        mut read: impl FnMut(&ResultView<'_>) -> T,
    ) -> Result<Option<T>, ClientError> {
        let mut outcome = None;
        self.drive(|core| {
            outcome = match core.next_batch(stream, now(), &mut read) {
                Ok(BatchPoll::Wait) => None,
                Ok(BatchPoll::Batch(value)) => Some(Ok(Some(value))),
                Ok(BatchPoll::Done) => Some(Ok(None)),
                Err(error) => Some(Err(error)),
            };
            outcome.is_some()
        })
        .await?;
        outcome.unwrap_or(Ok(None))
    }

    /// Cancels `stream`, and with it its transaction.
    pub async fn cancel(&mut self, stream: ResultStream) {
        self.core.cancel(stream);
        self.flush().await;
    }
}
