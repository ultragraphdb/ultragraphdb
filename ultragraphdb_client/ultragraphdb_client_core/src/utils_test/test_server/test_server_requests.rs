/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    Begin, BeginView, BodyBuilder, BodyView, ColumnType, Command, CommitView, ConditionalWriteView,
    Describe, DirectWriteView, DropCommandView, ErrorCode, ErrorReply, ExecuteView, FLAG_LAST,
    FrameView, Header, PatternSlot, Prepare, PrepareView, QuadPattern, ReadLevel, ReadView,
    Register, ResultBatch, ResultTerm, Welcome,
};

use crate::utils_test::session_record::SessionRecord;
use crate::utils_test::test_server::TestServer;
use crate::utils_test::test_server::test_transaction::{TestTransaction, WaitingRead};

impl TestServer {
    /// The header of the server's frame of `command` on `stream`, at the connection's protocol.
    fn header(&self, command: Command, stream: u64, flags: u8) -> Header {
        let protocol = self
            .connection
            .as_ref()
            .and_then(|c| c.welcome())
            .map_or(1, Welcome::protocol);
        Header::new(self.cluster, command)
            .with_stream(stream)
            .with_flags(flags)
            .with_release(self.welcome.release())
            .with_protocol(protocol)
    }

    /// `body` queued on `stream`, flagged `flags`.
    pub(super) fn reply<B: BodyBuilder>(&mut self, stream: u64, body: &B, flags: u8) {
        let Ok(frame) = body.frame(self.header(B::COMMAND, stream, flags)) else {
            return;
        };
        if let Some(connection) = self.connection.as_mut() {
            // A stream already ended, as a cancel ends it, takes no reply.
            drop(connection.outbound().push(stream, frame));
        }
    }

    /// An Error of `code` on `stream`, which ends it.
    pub(super) fn error(&mut self, stream: u64, code: ErrorCode, detail: u64, message: &str) {
        let error = ErrorReply::new(code, message).with_detail(detail);
        if let Some(connection) = self.connection.as_mut() {
            // A refusal of the test server's own short messages is always built; one that is not leaves the stream
            // as it was.
            drop(connection.refuse(stream, &error));
        }
    }

    /// A request the connection handed over, on its stream, answered.
    pub(super) fn request(&mut self, stream: u64, command: Command, frame: &ReceivedFrame) {
        let Ok(view) = frame.view() else {
            return;
        };
        match command {
            Command::Register => self.register(stream),
            Command::Begin => self.begin(stream, &view),
            Command::Operation => {
                if let Some(transaction) = self.transactions.get_mut(&stream) {
                    transaction.operations += 1;
                }
            }
            Command::Read => {
                if let Ok(read) = ReadView::from_frame(&view) {
                    self.answer_read(stream, read.transaction());
                }
            }
            Command::Execute => {
                if let Ok(execute) = ExecuteView::from_frame(&view) {
                    self.execute(stream, execute.statement(), execute.transaction());
                }
            }
            Command::Prepare => self.prepare(stream, &view),
            Command::Describe => {
                let answer = Describe::new(0, self.version);
                self.reply(stream, &answer, FLAG_LAST);
            }
            Command::Commit => self.commit_view(stream, &view),
            Command::Drop => {
                if let Ok(drop) = DropCommandView::from_frame(&view).and_then(|d| d.to_body()) {
                    self.drop(stream, drop);
                }
            }
            Command::Abort => {
                let _aborted: Option<TestTransaction> = self.transactions.remove(&stream);
            }
            Command::DirectWrite => self.direct_write(stream, &view),
            Command::ConditionalWrite => self.conditional_write(stream, &view),
            Command::Hello
            | Command::Welcome
            | Command::Write
            | Command::Query
            | Command::Install
            | Command::Result
            | Command::Credit
            | Command::Cancel
            | Command::Error
            | Command::Round
            | Command::Authenticate => {}
            // The client endpoint's connection refuses a role command before it reaches the server.
            Command::Join
            | Command::Members
            | Command::Subscribe
            | Command::EpochClose
            | Command::Lease
            | Command::Placement
            | Command::Catalog
            | Command::Challenge
            | Command::Verify
            | Command::Bind
            | Command::Recall
            | Command::ComputeRead
            | Command::Scan
            | Command::Entries
            | Command::Reference
            | Command::CloseRequest
            | Command::Closed
            | Command::DefaultEntry
            | Command::Report
            | Command::Held => {}
        }
    }

    /// A Commit `view` on `stream`, answered from its session and request.
    fn commit_view(&mut self, stream: u64, view: &FrameView<'_>) {
        if let Ok(commit) = CommitView::from_frame(view) {
            let session = u128::from_be_bytes(commit.session());
            self.commit(stream, session, commit.request());
        }
    }

    /// A DirectWrite `view` on `stream`, refused with Error 1 when it breaks a rule, as a declaration in it does.
    fn direct_write(&mut self, stream: u64, view: &FrameView<'_>) {
        match DirectWriteView::from_frame(view).and_then(|write| {
            write.check_from_client()?;
            Ok(write)
        }) {
            Ok(write) => {
                let session = u128::from_be_bytes(write.session());
                self.one_frame(stream, session, write.request(), 0);
            }
            Err(rule) => self.protocol_refusal(stream, &rule),
        }
    }

    /// A ConditionalWrite `view` on `stream`, each operation answered with the node it created.
    fn conditional_write(&mut self, stream: u64, view: &FrameView<'_>) {
        match ConditionalWriteView::from_frame(view).and_then(|write| {
            write.check_from_client()?;
            Ok(write)
        }) {
            Ok(write) => {
                let session = u128::from_be_bytes(write.session());
                let operations = u32::try_from(write.operation_count()).unwrap_or(u32::MAX);
                self.one_frame(stream, session, write.request(), operations);
            }
            Err(rule) => self.protocol_refusal(stream, &rule),
        }
    }

    /// Error 1 on `stream`, its message the rule's text.
    fn protocol_refusal(&mut self, stream: u64, rule: &ProtocolError) {
        if let Some(connection) = self.connection.as_mut() {
            // A rule's short text is always built.
            drop(connection.refuse(stream, &ErrorReply::protocol(rule)));
        }
    }

    /// A Prepare `view` on `stream`, answered with the next statement id, prepared against the current schema id,
    /// its result types those of the pattern's unbound slots.
    fn prepare(&mut self, stream: u64, view: &FrameView<'_>) {
        if let Ok(body) = PrepareView::from_frame(view).and_then(|p| p.to_body()) {
            let Some(pattern) = body.pattern().copied() else {
                return self.error(stream, ErrorCode::Refused, 0, "a text is not served");
            };
            let id = self.statements.len() as u64 + 1;
            let _replaced: Option<u64> = self.statements.insert(id, self.schema);
            let types = pattern
                .slots()
                .iter()
                .zip(QuadPattern::slot_types())
                .filter(|(slot, _)| **slot == PatternSlot::Unbound)
                .map(|(_, kind)| kind)
                // The unbound slots, then the reifier column, a node (`PROTOCOL.md` section 7.1).
                .chain(std::iter::once(ColumnType::Node))
                .collect();
            let reply = Prepare::new(
                body.tenant(),
                id,
                self.schema,
                body.terms().clone(),
                pattern,
                body.parameter_types().to_vec(),
                types,
            );
            self.reply(stream, &reply, FLAG_LAST);
        }
    }

    /// Statement `statement` executed on `stream`: refused when unknown or prepared against another schema id,
    /// answered as a read otherwise.
    fn execute(&mut self, stream: u64, statement: u64, transaction: u64) {
        match self.statements.get(&statement).copied() {
            None => self.error(stream, ErrorCode::UnknownStatement, statement, "unknown"),
            Some(schema) if schema != self.schema => self.error(
                stream,
                ErrorCode::StaleStatement,
                statement,
                "stale statement",
            ),
            Some(_) => self.answer_read(stream, transaction),
        }
    }

    /// A read on `stream` in the transaction of stream `transaction`: at once when the transaction's token names no epoch
    /// past the closed one, or waiting on it until its deadline.
    fn answer_read(&mut self, stream: u64, transaction: u64) {
        let open = self.transactions.get(&transaction).copied();
        let epoch = open.and_then(|t| t.epoch);
        match (epoch, self.closed, open.and_then(|t| t.deadline)) {
            (Some(epoch), Some(closed), Some(deadline)) if epoch > closed => {
                self.waiting.push(WaitingRead {
                    stream,
                    epoch,
                    deadline,
                });
            }
            (Some(epoch), _, _) => self.results_at(stream, epoch),
            (None, closed, _) => self.results_at(stream, closed.unwrap_or(0)),
        }
    }

    /// A Register on `stream`, answered with a session of the server's numbering, from 9 up.
    fn register(&mut self, stream: u64) {
        let session = self.sessions.len() as u128 + 9;
        let _fresh: Option<SessionRecord> = self.sessions.insert(session, SessionRecord::default());
        self.reply(stream, &Register::new(session.to_be_bytes()), FLAG_LAST);
    }

    /// A Begin `view` on `stream`, answered with every field as sent and the read version a read-only snapshot's
    /// bound, else 0 (`PROTOCOL.md` §6.2); its transaction opened.
    fn begin(&mut self, stream: u64, view: &FrameView<'_>) {
        let Ok(begin) = BeginView::from_frame(view) else {
            return;
        };
        let deadline_ms = match begin.deadline_ms() {
            0 => self.welcome.transaction_ms(),
            ms => ms,
        };
        let _replaced: Option<TestTransaction> = self.transactions.insert(
            stream,
            TestTransaction {
                operations: 0,
                epoch: begin.token().map(|token| token.epoch().get()),
                deadline: self
                    .now
                    .and_then(|now| now.checked_add(Duration::from_millis(deadline_ms.into()))),
            },
        );
        let snapshot = begin.read_only() && begin.level() == ReadLevel::Snapshot;
        let read_version = if snapshot { self.version } else { 0 };
        let body = Begin::new(begin.tenant(), read_version, begin.read_only())
            .with_level(begin.level())
            .with_deadline_ms(begin.deadline_ms());
        let body = match begin.token() {
            Some(token) => body.with_token(token.clone()),
            None => body,
        };
        self.reply(stream, &body, 0);
    }

    /// The server's Result frames on `stream`, read at `epoch`: one column and the reifier column, two rows naming the same IRI and no
    /// reifier, the last frame flagged; with no frames configured, one flagged batch of no rows, a read that matches
    /// nothing.
    pub(super) fn results_at(&mut self, stream: u64, epoch: u64) {
        let term = ResultTerm::iri("http://ex.org/same");
        let refusal = self.read_refusal.take();
        let results = refusal.map_or(self.results, |r| r.batches);
        if results == 0 && refusal.is_none() {
            let batch = ResultBatch::new(2).with_read_at(epoch, 1);
            return self.reply(stream, &batch, FLAG_LAST);
        }
        for i in 0..results {
            // Only a stream's last batch holds a resume key, and a refused read has no last batch.
            let last = refusal.is_none() && i + 1 == results;
            let resume = if last { vec![i as u8] } else { Vec::new() };
            let mut batch = ResultBatch::new(2)
                .with_resume(resume)
                .with_read_at(epoch, 1);
            let rows = batch
                .push_row(&[Some(&term), None])
                .and_then(|()| batch.push_row(&[Some(&term), None]));
            if let Err(error) = rows {
                // A row of a term and no reifier fits a batch of two columns, so the stream is refused only if that
                // breaks.
                let message =
                    format!("a row of a term and no reifier fits a batch of two columns: {error}");
                return self.error(stream, ErrorCode::OperationFailed, 0, &message);
            }

            let flags = if last { FLAG_LAST } else { 0 };
            self.reply(stream, &batch, flags);
        }
        if let Some(refusal) = refusal {
            self.error(stream, refusal.code, refusal.detail, "scripted");
        }
    }
}
