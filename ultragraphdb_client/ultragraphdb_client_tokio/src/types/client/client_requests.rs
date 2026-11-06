/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_client_core::{
    BatchPoll, ClientError, Committed, Reply, ResultStream, Session, Statement, Transaction,
};
use ultragraphdb_protocol_transport::Login;
use ultragraphdb_protocol_wire::{
    Begin, ConditionalWrite, Describe, DirectWrite, DropCommand, Execute, Operation, Prepare, Read,
    ResultView, Write,
};

use crate::traits::connect::Connect;
use crate::types::client::{Client, now};

/// A reply of another kind than its request's: the core answers each request with its own kind.
fn mismatch() -> ClientError {
    ClientError::reply_mismatch("reply", 0u64, 0u64)
}

/// The answer of a commit, a drop or a one-frame write.
fn committed(reply: Reply) -> Result<Committed, ClientError> {
    match reply {
        Reply::Committed(committed) | Reply::Dropped(committed) => Ok(committed),
        Reply::Registered(_) | Reply::Begun(_) | Reply::Prepared(_) | Reply::Described(_) => {
            Err(mismatch())
        }
    }
}

impl<C: Connect, L: Login> Client<C, L> {
    /// Registers a session (D9), through which commits are sent and retried.
    pub async fn register(&mut self) -> Result<Session, ClientError> {
        self.ready().await?;
        let pending = self.core.register(now())?;
        match self.settle(pending).await? {
            Reply::Registered(session) => Ok(session),
            Reply::Begun(_)
            | Reply::Committed(_)
            | Reply::Dropped(_)
            | Reply::Prepared(_)
            | Reply::Described(_) => Err(mismatch()),
        }
    }

    /// A session kept from an earlier client, its next request numbered `next_request`; refused for a number that does
    /// not fit it, as [`ultragraphdb_client_core::ClientCore::resume_session`] says.
    pub fn resume_session(
        &mut self,
        number: u128,
        next_request: u64,
    ) -> Result<Session, ClientError> {
        self.core.resume_session(number, next_request)
    }

    /// Begins a transaction in `tenant`, read-only or read-write, at the default level and deadline, carrying no token: its
    /// commit is ordered after no earlier write of any session, so one that must follow a session's writes begins with that
    /// session's token by [`Client::begin_with`] (`PROTOCOL.md` §11.11).
    pub async fn begin(
        &mut self,
        tenant: u64,
        read_only: bool,
    ) -> Result<Transaction, ClientError> {
        self.begin_with(Begin::new(tenant, 0, read_only), None)
            .await
    }

    /// Begins the transaction `begin` names, its level, deadline, read version and tenant; through a session it carries
    /// the session's token, or `begin`'s own when that is of a higher epoch.
    pub async fn begin_with(
        &mut self,
        begin: Begin,
        session: impl Into<Option<Session>>,
    ) -> Result<Transaction, ClientError> {
        let session: Option<Session> = session.into();
        self.ready().await?;
        let pending = self.core.begin_with(begin, session, now())?;
        match self.settle(pending).await? {
            Reply::Begun(transaction) => Ok(transaction),
            Reply::Registered(_)
            | Reply::Committed(_)
            | Reply::Dropped(_)
            | Reply::Prepared(_)
            | Reply::Described(_) => Err(mismatch()),
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
    /// number, across reconnects, so the server applies it once; through no session a lost reply is an unknown outcome.
    /// It ends the reads still open: read one to its end first when its every row is wanted. The commit follows `session`'s
    /// earlier writes in the epochs' order only when the transaction began with that session's token.
    pub async fn commit(
        &mut self,
        transaction: Transaction,
        session: impl Into<Option<Session>>,
    ) -> Result<Committed, ClientError> {
        let pending = self.core.commit(transaction, session, now())?;
        committed(self.settle(pending).await?)
    }

    /// Writes `write` in one frame, unconditional changes committed as one transaction with no Begin, under the same request
    /// counter and in-doubt rules as a commit. A write computed from an earlier read is a [`Client::conditional_write`] or a
    /// read-write transaction: sent as a read and then this, a commit between them is a lost update the server cannot see.
    pub async fn direct_write(
        &mut self,
        write: DirectWrite,
        session: impl Into<Option<Session>>,
    ) -> Result<Committed, ClientError> {
        let session: Option<Session> = session.into();
        self.ready().await?;
        let pending = self.core.direct_write(write, session, now())?;
        committed(self.settle(pending).await?)
    }

    /// Writes `write` in one frame, its commit-time operations decided in commit order and their outcome in the reply: a
    /// failed condition is an answer, refused with code 8 and the value found, and is not retried.
    pub async fn conditional_write(
        &mut self,
        write: ConditionalWrite,
        session: impl Into<Option<Session>>,
    ) -> Result<Committed, ClientError> {
        let session: Option<Session> = session.into();
        self.ready().await?;
        let pending = self.core.conditional_write(write, session, now())?;
        committed(self.settle(pending).await?)
    }

    /// Drops what `command` names through `session`, on a stream of its own, answered with the drop's commit version;
    /// retried only through the session, as a commit is, so the server applies it once.
    pub async fn drop_command(
        &mut self,
        command: DropCommand,
        session: impl Into<Option<Session>>,
    ) -> Result<Committed, ClientError> {
        let session: Option<Session> = session.into();
        self.ready().await?;
        let pending = self.core.drop_command(command, session, now())?;
        match self.settle(pending).await? {
            Reply::Dropped(dropped) => Ok(dropped),
            Reply::Registered(_)
            | Reply::Begun(_)
            | Reply::Committed(_)
            | Reply::Prepared(_)
            | Reply::Described(_) => Err(mismatch()),
        }
    }

    /// Resolves `session`'s request in doubt by sending it again with its number: committed from the server's record, or
    /// refused when it was not applied, or in doubt still.
    pub async fn resolve(&mut self, session: Session) -> Result<Committed, ClientError> {
        self.ready().await?;
        let pending = self.core.resolve(session, now())?;
        committed(self.settle(pending).await?)
    }

    /// Prepares a statement.
    pub async fn prepare(&mut self, prepare: &Prepare) -> Result<Statement, ClientError> {
        self.ready().await?;
        let pending = self.core.prepare(prepare, now())?;
        match self.settle(pending).await? {
            Reply::Prepared(statement) => Ok(statement),
            Reply::Registered(_)
            | Reply::Begun(_)
            | Reply::Committed(_)
            | Reply::Dropped(_)
            | Reply::Described(_) => Err(mismatch()),
        }
    }

    /// `tenant`'s data dictionary.
    pub async fn describe(&mut self, tenant: u64) -> Result<Describe, ClientError> {
        self.ready().await?;
        let pending = self.core.describe(tenant, now())?;
        match self.settle(pending).await? {
            Reply::Described(describe) => Ok(describe),
            Reply::Registered(_)
            | Reply::Begun(_)
            | Reply::Committed(_)
            | Reply::Dropped(_)
            | Reply::Prepared(_) => Err(mismatch()),
        }
    }

    /// Reads a quad pattern in `transaction`: a result stream, whose rows count only once it ends in `LAST`. Read it to its
    /// end, or cancel it, before waiting on a later read behind a write: that write waits for this read's `LAST`, and only
    /// this client's [`Client::next_batch`] drains it, by the credit it grants as batches are consumed.
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

    /// The next batch of `stream`, read in place by `read`, its credit granted as batches are consumed; `None` after the
    /// last. A refusal after rows is the read's failure, whatever batches it handed over, and nothing of it is resent.
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

    /// Cancels `stream`, a read; its transaction goes on.
    pub async fn cancel(&mut self, stream: ResultStream) {
        self.core.cancel(stream);
        self.flush().await;
    }
}
