/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_protocol_wire::DropCommand;

use crate::errors::client_error::ClientError;
use crate::types::reply::Reply;

/// What a request waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expect {
    Register,
    Begin,
    Commit { session: u64, request: u64 },
    Drop { session: u64, request: u64 },
    Prepare,
    Describe,
}

/// A request in flight: its stream, `None` while a commit waits to be sent again; the connection's sequence its frame
/// leaves with, which tells whether it has left; what it waits for; its answer once arrived; for a commit, the times
/// it was lost or answered busy, when it is sent again after a busy answer, and whether it was sent again, on a
/// stream of its own, through its session; for a drop, the Drop it sends again.
#[derive(Debug)]
pub(crate) struct PendingState {
    pub stream: Option<u64>,
    pub sequence: Option<u128>,
    pub expect: Expect,
    pub outcome: Option<Result<Reply, ClientError>>,
    pub tries: u32,
    pub resend_at: Option<Instant>,
    pub retried: bool,
    pub drop: Option<DropCommand>,
}

impl PendingState {
    pub(crate) fn new(stream: Option<u64>, expect: Expect) -> Self {
        Self {
            stream,
            sequence: None,
            expect,
            outcome: None,
            tries: 0,
            resend_at: None,
            retried: false,
            drop: None,
        }
    }

    /// The session and request of a commit or a drop, each retried through its session.
    pub(crate) fn commit(&self) -> Option<(u64, u64)> {
        match self.expect {
            Expect::Commit { session, request } | Expect::Drop { session, request } => {
                Some((session, request))
            }
            _ => None,
        }
    }

    /// Whether it waits for an answer on a stream of the connection.
    pub(crate) fn in_flight(&self) -> bool {
        self.outcome.is_none() && self.stream.is_some()
    }
}
