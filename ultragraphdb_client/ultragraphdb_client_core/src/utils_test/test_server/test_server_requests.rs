/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    Begin, BeginView, BodyBuilder, BodyView, ColumnType, Command, CommitView, Describe,
    DropCommandView, ErrorCode, ErrorReply, ExecuteView, FLAG_LAST, FrameView, Header, PatternSlot,
    Prepare, PrepareView, QuadPattern, ReadLevel, Register, ResultBatch, ResultTerm, Welcome,
};

use crate::utils_test::session_record::SessionRecord;
use crate::utils_test::test_server::TestServer;

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
                if let Some(operations) = self.transactions.get_mut(&stream) {
                    *operations += 1;
                }
            }
            Command::Read => self.results(stream),
            Command::Execute => {
                if let Ok(execute) = ExecuteView::from_frame(&view) {
                    self.execute(stream, execute.statement());
                }
            }
            Command::Prepare => self.prepare(stream, &view),
            Command::Describe => {
                let answer = Describe::new(0, self.version);
                self.reply(stream, &answer, FLAG_LAST);
            }
            Command::Commit => {
                if let Ok(commit) = CommitView::from_frame(&view) {
                    self.commit(
                        stream,
                        u128::from_be_bytes(commit.session()),
                        commit.request(),
                    );
                }
            }
            Command::Drop => {
                if let Ok(drop) = DropCommandView::from_frame(&view).and_then(|d| d.to_body()) {
                    self.drop(stream, drop);
                }
            }
            Command::Abort => {
                let _aborted: Option<u32> = self.transactions.remove(&stream);
            }
            Command::Hello
            | Command::Welcome
            | Command::Write
            | Command::DirectWrite
            | Command::ConditionalWrite
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
            | Command::Report => {}
        }
    }

    /// A Prepare `view` on `stream`, answered with the next statement id, prepared against the current schema id,
    /// its result types those of the pattern's unbound slots.
    fn prepare(&mut self, stream: u64, view: &FrameView<'_>) {
        if let Ok(body) = PrepareView::from_frame(view).and_then(|p| p.to_body()) {
            let id = self.statements.len() as u64 + 1;
            let _replaced: Option<u64> = self.statements.insert(id, self.schema);
            let types = body
                .pattern()
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
                *body.pattern(),
                body.parameter_types().to_vec(),
                types,
            );
            self.reply(stream, &reply, FLAG_LAST);
        }
    }

    /// Statement `statement` executed on `stream`: refused when unknown or prepared against another schema id,
    /// answered as a read otherwise.
    fn execute(&mut self, stream: u64, statement: u64) {
        match self.statements.get(&statement).copied() {
            None => self.error(stream, ErrorCode::UnknownStatement, statement, "unknown"),
            Some(schema) if schema != self.schema => self.error(
                stream,
                ErrorCode::StaleStatement,
                statement,
                "stale statement",
            ),
            Some(_) => self.results(stream),
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
        let _replaced: Option<u32> = self.transactions.insert(stream, 0);
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

    /// The server's Result frames on `stream`: one column and the reifier column, two rows naming the same IRI and no
    /// reifier, the last frame flagged; with no frames configured, one flagged batch of no rows, a read that matches
    /// nothing.
    fn results(&mut self, stream: u64) {
        let term = ResultTerm::iri("http://ex.org/same");
        let results = self.results;
        if results == 0 {
            return self.reply(stream, &ResultBatch::new(2), FLAG_LAST);
        }
        for i in 0..results {
            let mut batch = ResultBatch::new(2).with_resume(vec![i as u8]);
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

            let flags = if i + 1 == results { FLAG_LAST } else { 0 };
            self.reply(stream, &batch, flags);
        }
    }
}
