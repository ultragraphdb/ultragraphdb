/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod commit_error_display;
mod commit_error_error;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{Action, ConflictKind, ErrorCode};

/// Why the commit step did not commit a transaction: the reason its reply names and its Round decision records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitError(pub CommitErrorEnum);

/// The reasons, each with what its reply names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitErrorEnum {
    /// A transaction committed above the read version wrote what this one read: `kind` names the kind of read, or
    /// [`ConflictKind::Drop`] when the write was a range. Retryable.
    Conflict { kind: ConflictKind },
    /// The read version is below `floor`, the version the conflict history is pruned to. Retryable.
    TooOld { read_version: u64, floor: u64 },
    /// The transaction's write-set record, `size` bytes as it arrived or as its commit-time operations left it, is over
    /// `limit`, the largest a round holds on its own ([`crate::RoundBounds::max_write_set`]), the Welcome's
    /// `max_write_set`.
    TooLarge { size: usize, limit: usize },
    /// The write-set record does not read as one: a worker built it wrongly.
    Malformed { cause: ProtocolError },
    /// The read set was handed over unsealed, so its tables are not in the order validation walks them.
    Unsealed,
    /// The commit queue holds `limit` requests already. Retryable.
    Busy { limit: usize },
    /// The node refuses writes, after a failed log sync or once commit versions are exhausted, until it restarts.
    Unavailable,
    /// The commit names a session the session table does not hold: evicted, or never registered. The client registers
    /// a new session.
    SessionEvicted { session: u64 },
    /// The commit's request number is not one above its session's last committed one, nor that one: `expected` is.
    RequestOutOfOrder { expected: u64 },
    /// The session already has a commit in flight; its reply comes first.
    SessionBusy { session: u64 },
    /// A retry of `request`, with no transaction open for it, that the session table does not hold: the request was not
    /// committed, and the client runs its transaction again. Retryable.
    NotCommitted { request: u64 },
    /// Session `session` belongs to another principal: only the principal that registered it commits through it or
    /// reads its stored reply.
    PermissionDenied { session: u64 },
    /// A commit-time compare-and-set whose expected value was not held, or a counter add that could not add: the whole
    /// transaction is refused and wrote nothing; `found` is the value held, an N-Triples term, `None` when none is.
    OperationFailed { found: Option<String> },
    /// The catalog or the store refused a commit-time operation's write, as `reason` states; the transaction wrote
    /// nothing.
    Refused { reason: String },
}

impl CommitError {
    #[allow(non_snake_case)]
    pub fn Conflict(kind: ConflictKind) -> Self {
        Self(CommitErrorEnum::Conflict { kind })
    }

    #[allow(non_snake_case)]
    pub fn TooOld(read_version: u64, floor: u64) -> Self {
        Self(CommitErrorEnum::TooOld {
            read_version,
            floor,
        })
    }

    #[allow(non_snake_case)]
    pub fn TooLarge(size: usize, limit: usize) -> Self {
        Self(CommitErrorEnum::TooLarge { size, limit })
    }

    #[allow(non_snake_case)]
    pub fn Malformed(cause: ProtocolError) -> Self {
        Self(CommitErrorEnum::Malformed { cause })
    }

    #[allow(non_snake_case)]
    pub fn Unsealed() -> Self {
        Self(CommitErrorEnum::Unsealed)
    }

    #[allow(non_snake_case)]
    pub fn Unavailable() -> Self {
        Self(CommitErrorEnum::Unavailable)
    }

    #[allow(non_snake_case)]
    pub fn Busy(limit: usize) -> Self {
        Self(CommitErrorEnum::Busy { limit })
    }

    #[allow(non_snake_case)]
    pub fn SessionEvicted(session: u64) -> Self {
        Self(CommitErrorEnum::SessionEvicted { session })
    }

    #[allow(non_snake_case)]
    pub fn RequestOutOfOrder(expected: u64) -> Self {
        Self(CommitErrorEnum::RequestOutOfOrder { expected })
    }

    #[allow(non_snake_case)]
    pub fn SessionBusy(session: u64) -> Self {
        Self(CommitErrorEnum::SessionBusy { session })
    }

    #[allow(non_snake_case)]
    pub fn NotCommitted(request: u64) -> Self {
        Self(CommitErrorEnum::NotCommitted { request })
    }

    #[allow(non_snake_case)]
    pub fn PermissionDenied(session: u64) -> Self {
        Self(CommitErrorEnum::PermissionDenied { session })
    }

    #[allow(non_snake_case)]
    pub fn OperationFailed(found: Option<String>) -> Self {
        Self(CommitErrorEnum::OperationFailed { found })
    }

    #[allow(non_snake_case)]
    pub fn Refused(reason: String) -> Self {
        Self(CommitErrorEnum::Refused { reason })
    }

    /// The value a failed compare-and-set found, the Error frame's value: an N-Triples term, `None` when the failure
    /// is another or no value was held.
    pub fn found(&self) -> Option<&str> {
        match &self.0 {
            CommitErrorEnum::OperationFailed { found } => found.as_deref(),
            _ => None,
        }
    }

    /// The Error frame's code, which the Round decision records as its outcome.
    pub fn code(&self) -> ErrorCode {
        match self.0 {
            CommitErrorEnum::Conflict { .. } => ErrorCode::Conflict,
            CommitErrorEnum::TooOld { .. } => ErrorCode::TooOld,
            CommitErrorEnum::TooLarge { .. } => ErrorCode::TooLarge,
            CommitErrorEnum::Malformed { .. } | CommitErrorEnum::Unsealed => ErrorCode::Protocol,
            CommitErrorEnum::Unavailable => ErrorCode::Unavailable,
            CommitErrorEnum::Busy { .. } => ErrorCode::Busy,
            CommitErrorEnum::SessionEvicted { .. } => ErrorCode::SessionEvicted,
            CommitErrorEnum::RequestOutOfOrder { .. } => ErrorCode::RequestOutOfOrder,
            CommitErrorEnum::SessionBusy { .. } => ErrorCode::SessionBusy,
            CommitErrorEnum::NotCommitted { .. } => ErrorCode::NotCommitted,
            CommitErrorEnum::PermissionDenied { .. } => ErrorCode::PermissionDenied,
            CommitErrorEnum::OperationFailed { .. } => ErrorCode::OperationFailed,
            CommitErrorEnum::Refused { .. } => ErrorCode::Refused,
        }
    }

    /// The code's detail: the conflict's kind, the too-large limit in bytes, the session evicted or busy, the request
    /// number expected or not committed, the action denied, write; zero otherwise. The too-large limit is the largest
    /// record a round holds, the Welcome's `max_write_set`.
    pub fn detail(&self) -> u64 {
        match self.0 {
            CommitErrorEnum::Conflict { kind } => kind.number(),
            CommitErrorEnum::TooLarge { limit, .. } => limit as u64,
            CommitErrorEnum::SessionEvicted { session }
            | CommitErrorEnum::SessionBusy { session } => session,
            CommitErrorEnum::RequestOutOfOrder { expected } => expected,
            CommitErrorEnum::NotCommitted { request } => request,
            CommitErrorEnum::PermissionDenied { .. } => Action::Write.number(),
            _ => 0,
        }
    }

    /// Whether a retry of the transaction may commit: a conflict, too old, busy, session busy (after a delay) and not
    /// committed are, as `PROTOCOL.md` §9 marks their codes.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.0,
            CommitErrorEnum::Conflict { .. }
                | CommitErrorEnum::TooOld { .. }
                | CommitErrorEnum::Busy { .. }
                | CommitErrorEnum::SessionBusy { .. }
                | CommitErrorEnum::NotCommitted { .. }
        )
    }
}
