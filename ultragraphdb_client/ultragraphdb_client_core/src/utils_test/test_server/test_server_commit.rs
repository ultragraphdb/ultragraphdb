/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_wire::{
    Commit, DropCommand, ErrorCode, ErrorReply, FLAG_LAST, OperationResult,
};

use crate::utils_test::session_record::SessionRecord;
use crate::utils_test::test_server::TestServer;

impl TestServer {
    /// A scripted answer for the request about to be decided, applying it first when scripted so; whether it answered.
    fn scripted(&mut self, stream: u64, session: u128, request: u64, operations: u32) -> bool {
        let Some(answer) = self.answers.pop_front() else {
            return false;
        };
        if answer.apply {
            let _applied: SessionRecord = self.apply(session, request, operations);
        }
        let mut error = ErrorReply::new(answer.code, "scripted").with_detail(answer.detail);
        if let Some(value) = answer.value {
            error = error.with_value(value);
        }
        if let Some(connection) = self.connection.as_mut() {
            // A scripted refusal's short message is always built.
            drop(connection.refuse(stream, &error));
        }
        true
    }

    /// Commit `request` of `session` on `stream`, answered from the session table as `PROTOCOL.md` §11.5 has it: a
    /// retry of the last committed request by its stored reply, a number below the last refused with code 13, a request
    /// with no transaction open for it with code 18, an unknown session with code 12; otherwise applied. A commit through
    /// no session applies with no record, and its retry cannot be asked.
    pub(super) fn commit(&mut self, stream: u64, session: u128, request: u64) {
        let transaction = self.transactions.remove(&stream);
        let operations = transaction.map_or(0, |t| t.operations);
        if transaction.is_none() && self.busy_retries > 0 {
            self.busy_retries -= 1;
            return self.error(
                stream,
                ErrorCode::SessionBusy,
                detail(session),
                "a commit in flight",
            );
        }
        if transaction.is_none() && self.lost_retries > 0 {
            self.lost_retries -= 1;
            self.dropped = true;
            return;
        }
        if self.scripted(stream, session, request, operations) {
            return;
        }
        if session == 0 {
            return self.commit_sessionless(stream, operations);
        }
        let Some(record) = self.sessions.get(&session).cloned() else {
            return self.error(
                stream,
                ErrorCode::SessionEvicted,
                detail(session),
                "no such session",
            );
        };
        if transaction.is_none() && request == record.last && request > 0 {
            let reply = committed(session, request, &record);
            return self.reply(stream, &reply, FLAG_LAST);
        }
        if request < record.last || (transaction.is_some() && request == record.last) {
            return self.error(
                stream,
                ErrorCode::RequestOutOfOrder,
                record.last,
                "out of order",
            );
        }
        if transaction.is_none() {
            return self.error(stream, ErrorCode::NotCommitted, request, "not committed");
        }
        if self.lose_commit {
            self.lose_commit = false;
            self.dropped = true;
            return;
        }
        let applied = self.apply(session, request, operations);
        if self.drop_after_commit {
            self.drop_after_commit = false;
            self.dropped = true;
            return;
        }
        let reply = committed(session, request, &applied);
        self.reply(stream, &reply, FLAG_LAST);
    }

    /// A commit through no session: applied with no record, its reply holding the zero token.
    fn commit_sessionless(&mut self, stream: u64, operations: u32) {
        if self.lose_commit {
            self.lose_commit = false;
            self.dropped = true;
            return;
        }
        let applied = self.apply(0, 0, operations);
        if self.drop_after_commit {
            self.drop_after_commit = false;
            self.dropped = true;
            return;
        }
        self.reply(stream, &committed(0, 0, &applied), FLAG_LAST);
    }

    /// A DirectWrite or ConditionalWrite of `operations` through `session` on `stream`: its frame holds its changes, so a
    /// resend is decided by the table: the stored reply for its last request, code 13 below its last, and otherwise it
    /// commits (`PROTOCOL.md` §6.6).
    pub(super) fn one_frame(&mut self, stream: u64, session: u128, request: u64, operations: u32) {
        if self.busy_retries > 0 {
            self.busy_retries -= 1;
            return self.error(stream, ErrorCode::SessionBusy, 0, "a commit in flight");
        }
        if self.scripted(stream, session, request, operations) {
            return;
        }
        if session != 0 {
            let Some(record) = self.sessions.get(&session).cloned() else {
                return self.error(stream, ErrorCode::SessionEvicted, 0, "no such session");
            };
            if request == record.last && request > 0 {
                let reply = committed(session, request, &record);
                return self.reply(stream, &reply, FLAG_LAST);
            }
            if request < record.last {
                return self.error(stream, ErrorCode::RequestOutOfOrder, record.last, "order");
            }
        }
        let applied = self.apply(session, request, operations);
        if self.drop_after_commit {
            self.drop_after_commit = false;
            self.dropped = true;
            return;
        }
        let reply = committed(session, request, &applied);
        self.reply(stream, &reply, FLAG_LAST);
    }

    /// `command`, a Drop, on `stream`, answered from the session table as a one-frame write is and applied as one: its
    /// reply the drop with its commit version. The drop removes nothing here: the server holds no data.
    pub(super) fn drop(&mut self, stream: u64, command: DropCommand) {
        let (session, request) = (u128::from_be_bytes(command.session()), command.request());
        if self.scripted(stream, session, request, 0) {
            return;
        }
        if session != 0 {
            let Some(record) = self.sessions.get(&session).cloned() else {
                return self.error(
                    stream,
                    ErrorCode::SessionEvicted,
                    detail(session),
                    "no such session",
                );
            };
            if request == record.last && request > 0 {
                let reply = dropped(command, session, &record);
                return self.reply(stream, &reply, FLAG_LAST);
            }
            if request < record.last {
                return self.error(
                    stream,
                    ErrorCode::RequestOutOfOrder,
                    record.last,
                    "out of order",
                );
            }
        }
        let applied = self.apply(session, request, 0);
        if self.drop_after_commit {
            self.drop_after_commit = false;
            self.dropped = true;
            return;
        }
        self.reply(stream, &dropped(command, session, &applied), FLAG_LAST);
    }

    /// Request `request` of `session` applied with its `operations`, each answered with the node it created, by its index in the reply's term table; through a session, recorded.
    fn apply(&mut self, session: u128, request: u64, operations: u32) -> SessionRecord {
        self.version += 2;
        self.applied += 1;
        let position = self.position.take().unwrap_or(self.version);
        let record = SessionRecord {
            last: request,
            version: self.version,
            token: (session != 0).then(|| token_at(position, self.version)),
            results: (0..operations)
                .map(|i| OperationResult::for_node(i, true))
                .collect(),
        };
        if session != 0 {
            let _previous: Option<SessionRecord> = self.sessions.insert(session, record.clone());
        }

        record
    }
}

/// The Commit reply to `request` of `session`, from its `record`: a result naming a node names it in the reply's term
/// table, by an IRI of its own (`PROTOCOL.md` §6.3).
fn committed(session: u128, request: u64, record: &SessionRecord) -> Commit {
    let mut reply = Commit::new(session.to_be_bytes(), request, record.version);
    // A reply through no session holds the zero token (`PROTOCOL.md` §6.3).
    if let Some(token) = &record.token {
        reply = reply.with_token(token.clone());
    }
    for result in &record.results {
        match result.node() {
            Some(node) => {
                reply.push_node_result(&format!("http://ex.org/node/{node}"), result.created())
            }
            None => reply.push_result(*result),
        }
    }
    reply
}

/// The reply to `command`, a Drop through `session`, from its `record`: through no session, the zero token.
fn dropped(command: DropCommand, session: u128, record: &SessionRecord) -> DropCommand {
    let reply = command.with_commit_version(record.version);
    match (&record.token, session) {
        (Some(token), 1..) => reply.with_token(token.clone()),
        (Some(_) | None, _) => reply,
    }
}

/// `session` as an Error's detail, which holds 64 bits: the test server numbers its sessions from 9 up.
fn detail(session: u128) -> u64 {
    u64::try_from(session).unwrap_or(u64::MAX)
}

/// The token of a commit at `version`: partition 1, its position the version, its epoch the version's
/// (`PROTOCOL.md` §6.3). The test server's versions start in epoch 1.
pub(super) fn token(version: u64) -> SessionToken {
    token_at(version, version)
}

/// The token of a commit at `version` whose round is at `position` of partition 1.
fn token_at(position: u64, version: u64) -> SessionToken {
    SessionToken::new(
        1,
        position,
        Epoch::new(version >> 22).unwrap_or(Epoch::ZERO),
    )
}
