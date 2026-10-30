/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    Begin, BeginView, BodyBuilder, BodyView, Command, CommitView, Describe, DropCommandView,
    ErrorCode, ErrorReply, ExecuteView, FLAG_LAST, Header, PatternSlot, Prepare, PrepareView,
    QuadPattern, Register, ResultBatch, ResultTerm, Welcome,
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
            let _ = connection.outbound().push(stream, frame);
        }
    }

    /// An Error of `code` on `stream`, which ends it.
    pub(super) fn error(&mut self, stream: u64, code: ErrorCode, detail: u64, message: &str) {
        let error = ErrorReply::new(code, message).with_detail(detail);
        if let Some(connection) = self.connection.as_mut() {
            connection.refuse(stream, &error);
        }
    }

    /// A request the connection handed over, on its stream, answered.
    pub(super) fn request(&mut self, stream: u64, command: Command, frame: &ReceivedFrame) {
        let Ok(view) = frame.view() else {
            return;
        };
        match command {
            Command::Register => {
                let session = self.sessions.len() as u64 + 9;
                self.sessions.insert(session, SessionRecord::default());
                self.reply(stream, &Register::new(session), FLAG_LAST);
            }
            Command::Begin => {
                if let Ok(begin) = BeginView::from_frame(&view) {
                    self.transactions.insert(stream, 0);
                    let body = Begin::new(begin.namespace(), self.version, begin.read_only());
                    self.reply(stream, &body, 0);
                }
            }
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
            Command::Prepare => {
                if let Ok(body) = PrepareView::from_frame(&view).and_then(|p| p.to_body()) {
                    let id = self.statements.len() as u64 + 1;
                    self.statements.insert(id, self.schema);
                    let types = body
                        .pattern()
                        .slots()
                        .iter()
                        .zip(QuadPattern::slot_types())
                        .filter(|(slot, _)| **slot == PatternSlot::Unbound)
                        .map(|(_, kind)| kind)
                        .collect();
                    let reply = Prepare::new(
                        body.namespace(),
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
            Command::Describe => {
                let answer = Describe::new(0, self.version);
                self.reply(stream, &answer, FLAG_LAST);
            }
            Command::Commit => {
                if let Ok(commit) = CommitView::from_frame(&view) {
                    self.commit(stream, commit.session(), commit.request());
                }
            }
            Command::Drop => {
                if let Ok(drop) = DropCommandView::from_frame(&view).and_then(|d| d.to_body()) {
                    self.drop(stream, drop);
                }
            }
            Command::Abort => {
                self.transactions.remove(&stream);
            }
            _ => {}
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

    /// The server's Result frames on `stream`: one column, two rows naming the same IRI, the last frame flagged; with
    /// no frames configured, one flagged batch of no rows, a read that matches nothing.
    fn results(&mut self, stream: u64) {
        let term = ResultTerm::new(EncodedTerm::uid(42)).with_text("http://ex.org/same");
        let results = self.results;
        if results == 0 {
            return self.reply(stream, &ResultBatch::new(1), FLAG_LAST);
        }
        for i in 0..results {
            let mut batch = ResultBatch::new(1).with_resume(vec![i as u8]);
            let _ = batch.push_row(&[Some(&term)]);
            let _ = batch.push_row(&[Some(&term)]);
            let flags = if i + 1 == results { FLAG_LAST } else { 0 };
            self.reply(stream, &batch, flags);
        }
    }
}
