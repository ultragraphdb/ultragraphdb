/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A scripted server for the client's tests, sans-I/O as the client is: it reads whole frames and queues the frames
//! it answers with, as the node will (group 10), from a session table that answers a retried commit from its record.

use std::collections::{BTreeMap, VecDeque};

use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_transport::{Outbound, ServerHandshake};
use ultragraphdb_protocol_wire::{
    BeginView, BodyBuilder, BodyView, Command, Commit, CommitView, Describe, ErrorCode, ErrorReply,
    ExecuteView, FLAG_LAST, FrameView, Header, MAX_FRAME_SIZE, OperationResult, PrepareView,
    Register, ResultBatch, ResultTerm, Welcome,
};

pub use crate::utils_test::server_script::ServerScript;

/// A session's last committed request and its reply's version and results.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Record {
    last: u64,
    version: u64,
    results: Vec<OperationResult>,
}

/// A commit applied with its reply withheld: its session, request and operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Withheld {
    session: u64,
    request: u64,
    operations: u32,
}

/// A scripted server: the handshake through [`ServerHandshake`], then Register, Begin, Write, Operation, Read,
/// Prepare, Execute, Describe, Commit, Abort, Credit and Cancel answered by its script, replies scheduled by
/// [`Outbound`]. Sessions, statements and the commits applied outlive a connection; transactions do not.
#[derive(Debug, Clone)]
pub struct ScriptedServer {
    cluster: u64,
    welcome: Welcome,
    script: ServerScript,
    welcomed: bool,
    direct: VecDeque<Vec<u8>>,
    outbound: Outbound,
    sessions: BTreeMap<u64, Record>,
    transactions: BTreeMap<u64, u32>,
    statements: BTreeMap<u64, u64>,
    withheld: Option<Withheld>,
    received: Vec<Vec<u8>>,
    applied: u64,
    version: u64,
    dropped: bool,
}

impl ScriptedServer {
    /// A server of `cluster` welcoming as `welcome` and answering by `script`.
    pub fn new(cluster: u64, welcome: Welcome, script: ServerScript) -> Self {
        let outbound = Outbound::new(welcome.max_streams(), welcome.initial_credit());
        Self {
            cluster,
            welcome,
            script,
            welcomed: false,
            direct: VecDeque::new(),
            outbound,
            sessions: BTreeMap::new(),
            transactions: BTreeMap::new(),
            statements: BTreeMap::new(),
            withheld: None,
            received: Vec::new(),
            applied: 0,
            version: 998,
            dropped: false,
        }
    }

    /// A new connection: the handshake again, no transaction or queued frame kept.
    pub fn connection(&mut self) {
        self.welcomed = false;
        self.dropped = false;
        self.direct.clear();
        self.outbound = Outbound::new(self.welcome.max_streams(), self.welcome.initial_credit());
        self.transactions.clear();
    }

    /// Whether the server dropped the connection, after a commit it withholds the reply of.
    pub fn dropped(&self) -> bool {
        self.dropped
    }

    /// The commits applied, each once.
    pub fn applied(&self) -> u64 {
        self.applied
    }

    /// Every frame received, as its bytes.
    pub fn received(&self) -> &[Vec<u8>] {
        &self.received
    }

    /// The namespace's schema id changes, as a catalog change changes it.
    pub fn set_schema(&mut self, schema: u64) {
        self.script.schema = schema;
    }

    /// The next frame to send.
    pub fn transmit(&mut self) -> Option<Vec<u8>> {
        if self.dropped {
            return None;
        }
        self.direct
            .pop_front()
            .or_else(|| self.outbound.next_frame().map(|(_, frame)| frame))
    }

    fn header(&self, command: Command, stream: u64, flags: u8) -> Header {
        Header::new(self.cluster, command)
            .with_stream(stream)
            .with_flags(flags)
            .with_release(self.welcome.release())
    }

    fn push(&mut self, stream: u64, frame: Vec<u8>) {
        if !self.outbound.is_open(stream) {
            let _ = self.outbound.open(stream);
        }
        let _ = self.outbound.push(stream, frame);
    }

    fn reply<B: BodyBuilder>(&mut self, stream: u64, body: &B, flags: u8) {
        if let Ok(frame) = body.frame(self.header(B::COMMAND, stream, flags)) {
            self.push(stream, frame);
        }
    }

    fn error(&mut self, stream: u64, code: ErrorCode, detail: u64, message: &str) {
        let error = ErrorReply::new(code, message).with_detail(detail);
        self.reply(stream, &error, FLAG_LAST);
    }

    /// Reads one whole frame of the client.
    pub fn receive(&mut self, frame: &[u8]) {
        if self.dropped {
            return;
        }
        self.received.push(frame.to_vec());
        if !self.welcomed {
            let (answer, outcome) = ServerHandshake::new(self.cluster, self.welcome.clone())
                .receive(frame)
                .into_parts();
            self.direct.extend(answer);
            self.welcomed = outcome.is_ok();
            return;
        }
        let Ok(view) = FrameView::validate_client(frame, self.cluster, MAX_FRAME_SIZE) else {
            return;
        };
        let stream = view.header().stream();
        match view.command() {
            Command::Register => {
                let session = self.sessions.len() as u64 + 9;
                self.sessions.insert(session, Record::default());
                self.reply(stream, &Register::new(session), 0);
            }
            Command::Begin => {
                if let Ok(begin) = BeginView::from_frame(&view) {
                    self.transactions.insert(stream, 0);
                    let body = ultragraphdb_protocol_wire::Begin::new(
                        begin.namespace(),
                        self.version,
                        begin.read_only(),
                    );
                    self.reply(stream, &body, 0);
                }
            }
            Command::Write => {}
            Command::Operation => {
                if let Some(operations) = self.transactions.get_mut(&stream) {
                    *operations += 1;
                }
            }
            Command::Read => self.results(stream),
            Command::Execute => {
                if let Ok(execute) = ExecuteView::from_frame(&view) {
                    match self.statements.get(&execute.statement()) {
                        None => self.error(
                            stream,
                            ErrorCode::UnknownStatement,
                            execute.statement(),
                            "unknown",
                        ),
                        Some(schema) if *schema != self.script.schema => self.error(
                            stream,
                            ErrorCode::StaleStatement,
                            execute.statement(),
                            "stale statement",
                        ),
                        Some(_) => self.results(stream),
                    }
                }
            }
            Command::Prepare => {
                if let Ok(body) = PrepareView::from_frame(&view).and_then(|p| p.to_body()) {
                    let id = self.statements.len() as u64 + 1;
                    self.statements.insert(id, self.script.schema);
                    let types = body
                        .pattern()
                        .slots()
                        .iter()
                        .zip(ultragraphdb_protocol_wire::QuadPattern::slot_types())
                        .filter(|(slot, _)| {
                            **slot == ultragraphdb_protocol_wire::PatternSlot::Unbound
                        })
                        .map(|(_, kind)| kind)
                        .collect();
                    let reply = ultragraphdb_protocol_wire::Prepare::new(
                        body.namespace(),
                        id,
                        self.script.schema,
                        body.terms().clone(),
                        *body.pattern(),
                        body.parameter_types().to_vec(),
                        types,
                    );
                    self.reply(stream, &reply, 0);
                }
            }
            Command::Describe => {
                let answer = Describe::new(0, self.version);
                self.reply(stream, &answer, 0);
            }
            Command::Commit => {
                if let Ok(commit) = CommitView::from_frame(&view) {
                    self.commit(stream, commit.session(), commit.request());
                }
            }
            Command::Abort => {
                self.transactions.remove(&stream);
            }
            Command::Credit => {
                if let Ok(credit) = ultragraphdb_protocol_wire::CreditView::from_frame(&view) {
                    self.outbound.grant(stream, credit.frames());
                }
            }
            Command::Cancel => {
                self.outbound.cancel(stream);
                self.transactions.remove(&stream);
            }
            _ => {}
        }
    }

    /// The script's Result frames on `stream`: one column, two rows naming the same IRI, the last frame flagged.
    fn results(&mut self, stream: u64) {
        let term = ResultTerm::new(EncodedTerm::uid(42)).with_text("http://ex.org/same");
        for i in 0..self.script.results {
            let mut batch = ResultBatch::new(1).with_resume(vec![i as u8]);
            let _ = batch.push_row(&[Some(&term)]);
            let _ = batch.push_row(&[Some(&term)]);
            let flags = if i + 1 == self.script.results {
                FLAG_LAST
            } else {
                0
            };
            self.reply(stream, &batch, flags);
        }
    }

    fn apply(&mut self, session: u64, request: u64, operations: u32) {
        self.version += 2;
        self.applied += 1;
        let results = (0..operations)
            .map(|i| OperationResult::new(100 + u64::from(i), true))
            .collect();
        self.sessions.insert(
            session,
            Record {
                last: request,
                version: self.version,
                results,
            },
        );
    }

    fn commit(&mut self, stream: u64, session: u64, request: u64) {
        let transaction = self.transactions.remove(&stream);
        if let Some(withheld) = self
            .withheld
            .filter(|w| w.session == session && w.request == request)
        {
            if self.script.busy_retries > 0 {
                self.script.busy_retries -= 1;
                return self.error(
                    stream,
                    ErrorCode::SessionBusy,
                    session,
                    "a commit in flight",
                );
            }
            self.withheld = None;
            self.apply(withheld.session, withheld.request, withheld.operations);
        }
        let Some(record) = self.sessions.get(&session).cloned() else {
            return self.error(
                stream,
                ErrorCode::SessionEvicted,
                session,
                "no such session",
            );
        };
        if request == record.last && request > 0 {
            let reply = Commit::new(session, request, record.version, record.results);
            return self.reply(stream, &reply, FLAG_LAST);
        }
        if request != record.last + 1 {
            return self.error(
                stream,
                ErrorCode::RequestOutOfOrder,
                record.last + 1,
                "out of order",
            );
        }
        let Some(operations) = transaction else {
            return self.error(
                stream,
                ErrorCode::Refused,
                request,
                "no transaction on the stream",
            );
        };
        if self.script.drop_commits > 0 {
            self.script.drop_commits -= 1;
            self.dropped = true;
            if self.script.busy_retries > 0 {
                self.withheld = Some(Withheld {
                    session,
                    request,
                    operations,
                });
            } else {
                self.apply(session, request, operations);
            }
            return;
        }
        self.apply(session, request, operations);
        let record = self.sessions.get(&session).cloned().unwrap_or_default();
        let reply = Commit::new(session, request, record.version, record.results);
        self.reply(stream, &reply, FLAG_LAST);
    }
}
