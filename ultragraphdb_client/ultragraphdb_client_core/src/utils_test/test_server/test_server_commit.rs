/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_wire::{Commit, DropCommand, ErrorCode, FLAG_LAST, OperationResult};

use crate::utils_test::session_record::SessionRecord;
use crate::utils_test::test_server::TestServer;

impl TestServer {
    /// Commit `request` of `session` on `stream`, answered from the session table as `PROTOCOL.md` §11.5 has it: a
    /// retry of the last committed request by its stored reply, another number than the next refused with code 13,
    /// a request with no transaction open for it with code 18, an unknown session with code 12; otherwise applied.
    pub(super) fn commit(&mut self, stream: u64, session: u128, request: u64) {
        let transaction = self.transactions.remove(&stream);
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
        let Some(record) = self.sessions.get(&session).cloned() else {
            return self.error(
                stream,
                ErrorCode::SessionEvicted,
                detail(session),
                "no such session",
            );
        };
        if request == record.last && request > 0 {
            let reply = committed(session, request, record.version, record.results);
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
            return self.error(stream, ErrorCode::NotCommitted, request, "not committed");
        };
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
        let reply = committed(session, request, applied.version, applied.results);
        self.reply(stream, &reply, FLAG_LAST);
    }

    /// `command`, a Drop, on `stream`, answered from the session table as a commit is and applied as one, though it
    /// needs no transaction: its reply the drop with its commit version. The drop removes nothing here: the server
    /// holds no data.
    pub(super) fn drop(&mut self, stream: u64, command: DropCommand) {
        let (session, request) = (u128::from_be_bytes(command.session()), command.request());
        let Some(record) = self.sessions.get(&session).cloned() else {
            return self.error(
                stream,
                ErrorCode::SessionEvicted,
                detail(session),
                "no such session",
            );
        };
        if request == record.last && request > 0 {
            let reply = dropped(command, session, record.version);
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
        let applied = self.apply(session, request, 0);
        if self.drop_after_commit {
            self.drop_after_commit = false;
            self.dropped = true;
            return;
        }
        self.reply(
            stream,
            &dropped(command, session, applied.version),
            FLAG_LAST,
        );
    }

    /// Request `request` of `session` applied with its `operations`, each answered with the node it created, by its index in the reply's term table.
    fn apply(&mut self, session: u128, request: u64, operations: u32) -> SessionRecord {
        self.version += 2;
        self.applied += 1;
        let record = SessionRecord {
            last: request,
            version: self.version,
            results: (0..operations)
                .map(|i| OperationResult::for_node(i, true))
                .collect(),
        };
        let _previous: Option<SessionRecord> = self.sessions.insert(session, record.clone());

        record
    }
}

/// The Commit reply to `request` of `session` at `version`, with the operations' `results`: a result naming a node
/// names it in the reply's term table, by an IRI of its own (`PROTOCOL.md` §6.3).
fn committed(session: u128, request: u64, version: u64, results: Vec<OperationResult>) -> Commit {
    let mut reply = Commit::new(session.to_be_bytes(), request, version);
    // A reply through no session holds the zero token (`PROTOCOL.md` §6.3).
    if session != 0 {
        reply = reply.with_token(token(version));
    }
    for result in results {
        match result.node() {
            Some(node) => {
                reply.push_node_result(&format!("http://ex.org/node/{node}"), result.created())
            }
            None => reply.push_result(result),
        }
    }
    reply
}

/// The reply to `command`, a Drop through `session` at `version`: through no session, the zero token.
fn dropped(command: DropCommand, session: u128, version: u64) -> DropCommand {
    let reply = command.with_commit_version(version);
    if session == 0 {
        return reply;
    }
    reply.with_token(token(version))
}

/// `session` as an Error's detail, which holds 64 bits: the test server numbers its sessions from 9 up.
fn detail(session: u128) -> u64 {
    u64::try_from(session).unwrap_or(u64::MAX)
}

/// The token of a commit at `version`: partition 1, its position the version, its epoch the version's
/// (`PROTOCOL.md` §6.3). The test server's versions start in epoch 1.
pub(super) fn token(version: u64) -> SessionToken {
    SessionToken::new(1, version, Epoch::new(version >> 22).unwrap_or(Epoch::ZERO))
}
