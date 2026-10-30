/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Commit, ErrorCode, FLAG_LAST, OperationResult};

use crate::utils_test::session_record::SessionRecord;
use crate::utils_test::test_server::TestServer;

impl TestServer {
    /// Commit `request` of `session` on `stream`, answered from the session table as `PROTOCOL.md` §11.5 has it: a
    /// retry of the last committed request by its stored reply, another number than the next refused with code 13,
    /// a request with no transaction open for it with code 18, an unknown session with code 12; otherwise applied.
    pub(super) fn commit(&mut self, stream: u64, session: u64, request: u64) {
        let transaction = self.transactions.remove(&stream);
        if transaction.is_none() && self.busy_retries > 0 {
            self.busy_retries -= 1;
            return self.error(
                stream,
                ErrorCode::SessionBusy,
                session,
                "a commit in flight",
            );
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
            return self.error(stream, ErrorCode::NotCommitted, request, "not committed");
        };
        if self.lose_commit {
            self.lose_commit = false;
            self.dropped = true;
            return;
        }
        let record = self.apply(session, request, operations);
        if self.drop_after_commit {
            self.drop_after_commit = false;
            self.dropped = true;
            return;
        }
        let reply = Commit::new(session, request, record.version, record.results);
        self.reply(stream, &reply, FLAG_LAST);
    }

    /// Request `request` of `session` applied with its `operations`, each answered with a UID it created.
    fn apply(&mut self, session: u64, request: u64, operations: u32) -> SessionRecord {
        self.version += 2;
        self.applied += 1;
        let record = SessionRecord {
            last: request,
            version: self.version,
            results: (0..operations)
                .map(|i| OperationResult::new(100 + u64::from(i), true))
                .collect(),
        };
        self.sessions.insert(session, record.clone());
        record
    }
}
