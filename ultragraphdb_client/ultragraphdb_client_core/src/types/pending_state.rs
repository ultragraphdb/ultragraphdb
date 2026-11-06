/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_protocol_wire::{Command, RefusedRequest};

use crate::errors::client_error::ClientError;
use crate::types::reply::Reply;
use crate::types::resend::Resend;
use crate::types::session_state::SessionState;

/// What a request waits for. A session 0 names none: a request through no session, numbered 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expect {
    Register,
    Begin,
    Commit { session: u128, request: u64 },
    Write { session: u128, request: u64 },
    Drop { session: u128, request: u64 },
    Prepare,
    Describe,
}

impl Expect {
    /// The command `PROTOCOL.md` §3 lists in the request's "Answered by", beside Error.
    pub(crate) fn answered_by(self) -> Command {
        match self {
            Expect::Register => Command::Register,
            Expect::Begin => Command::Begin,
            Expect::Commit { .. } | Expect::Write { .. } => Command::Commit,
            Expect::Drop { .. } => Command::Drop,
            Expect::Prepare => Command::Prepare,
            Expect::Describe => Command::Describe,
        }
    }

    /// The kind of request a refusal of it refused.
    pub(crate) fn refused_request(self) -> RefusedRequest {
        match self {
            Expect::Commit { .. } => RefusedRequest::Commit,
            Expect::Write { .. } | Expect::Drop { .. } => RefusedRequest::OneFrame,
            Expect::Register | Expect::Begin | Expect::Prepare | Expect::Describe => {
                RefusedRequest::Other
            }
        }
    }
}

/// A request in flight: its stream, `None` while a commit waits to be sent again; the connection's sequence its frame
/// leaves with, which tells whether it has left; what it waits for; its answer once arrived; for a commit, a drop or a
/// one-frame write, whether its frame ever left on any connection, the times it was lost or answered busy, when it is
/// sent again after such an answer, whether it was sent again, on a stream of its own, and what sends it again.
#[derive(Debug)]
pub(crate) struct PendingState {
    pub stream: Option<u64>,
    pub sequence: Option<u128>,
    pub expect: Expect,
    pub outcome: Option<Result<Reply, ClientError>>,
    pub ever_sent: bool,
    pub tries: u32,
    pub resend_at: Option<Instant>,
    pub retried: bool,
    pub resend: Resend,
}

impl PendingState {
    pub(crate) fn new(stream: Option<u64>, expect: Expect) -> Self {
        Self {
            stream,
            sequence: None,
            expect,
            outcome: None,
            ever_sent: false,
            tries: 0,
            resend_at: None,
            retried: false,
            resend: Resend::Commit,
        }
    }

    /// The session and request of a commit, a drop or a one-frame write, each sent through one request counter; session 0
    /// for none.
    pub(crate) fn commit(&self) -> Option<(u128, u64)> {
        match self.expect {
            Expect::Commit { session, request }
            | Expect::Write { session, request }
            | Expect::Drop { session, request } => Some((session, request)),
            Expect::Register | Expect::Begin | Expect::Prepare | Expect::Describe => None,
        }
    }

    /// Whether it waits for an answer on a stream of the connection.
    pub(crate) fn in_flight(&self) -> bool {
        self.outcome.is_none() && self.stream.is_some()
    }

    /// Its answer is that request `request` of `session` is in doubt for `cause`; the session, `state`, keeps what resends
    /// it, and commits nothing else until a resend resolves it.
    pub(crate) fn mark_in_doubt(
        &mut self,
        (session, request): (u128, u64),
        state: Option<&mut SessionState>,
        cause: Option<ClientError>,
    ) {
        debug_assert!(session != 0, "a request through no session has no resend");
        self.outcome = Some(Err(ClientError::in_doubt(session, request, cause)));
        if let Some(state) = state {
            state.in_flight = false;
            state.in_doubt = Some((request, std::mem::replace(&mut self.resend, Resend::Commit)));
        }
    }
}
