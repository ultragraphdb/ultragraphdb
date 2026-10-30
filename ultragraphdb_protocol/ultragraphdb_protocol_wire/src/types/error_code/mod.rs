/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Why a request was refused, the Error frame's `code`. A code's number is never reused for another meaning.
///
/// | Number | Code | Retryable | `detail` |
/// |---|---|---|---|
/// | 1 | Protocol | no | — |
/// | 2 | Conflict | yes | the [`crate::ConflictKind`] of the read that conflicted |
/// | 3 | TooOld | yes | — |
/// | 4 | PastLimit | yes | the limit, in milliseconds |
/// | 5 | Busy | yes | — |
/// | 6 | TooLarge | no | the limit, in bytes |
/// | 7 | Refused | no | —; for a term's rule, the term's index |
/// | 8 | OperationFailed | no | — (the value found is the frame's term table) |
/// | 9 | StaleStatement | no | the statement |
/// | 10 | UnknownStatement | no | the statement |
/// | 11 | Parameters | no | the statement; 0 for a parameter type |
/// | 12 | SessionEvicted | no | the session |
/// | 13 | RequestOutOfOrder | no | the request number expected |
/// | 14 | SessionBusy | yes, after a delay | the session |
/// | 15 | StreamLimit | no | the limit |
/// | 16 | ProtocolVersion | no | the server's highest protocol |
/// | 17 | Unavailable | no | — |
/// | 18 | NotCommitted | yes | the request |
/// | 19 | AuthenticationFailed | no | — |
/// | 20 | Unauthenticated | no | — |
/// | 21 | PermissionDenied | no | the [`crate::Action`] asked for |
/// | 22 | CredentialExpired | no | — |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ErrorCode {
    /// The frame broke a validation rule: a connection error, after which the connection closes, or a stream error,
    /// which ends the request's stream.
    Protocol,
    /// A transaction's read conflicted with a later-ordered commit.
    Conflict,
    /// A transaction's read version is below what validation still holds.
    TooOld,
    /// A transaction lived past its limit.
    PastLimit,
    /// The commit queue is full.
    Busy,
    /// A write set exceeds the Welcome's largest write set.
    TooLarge,
    /// The catalog refused a write, or a term broke the server's rules.
    Refused,
    /// A compare-and-set did not hold.
    OperationFailed,
    /// A prepared statement's schema id no longer matches its namespace's catalog.
    StaleStatement,
    /// No statement of the connection has the id executed.
    UnknownStatement,
    /// An execution's parameters do not fit its statement.
    Parameters,
    /// The session was evicted from the session table.
    SessionEvicted,
    /// A commit's request number is not one above its session's last.
    RequestOutOfOrder,
    /// The session already has a commit in flight.
    SessionBusy,
    /// The connection holds as many streams in flight as Welcome allows.
    StreamLimit,
    /// No protocol both sides speak.
    ProtocolVersion,
    /// The node refuses writes after a failed log sync, until it restarts.
    Unavailable,
    /// A retried commit the session table does not hold, with no transaction open: it was not committed, and the
    /// client runs its transaction again.
    NotCommitted,
    /// The authentication exchange failed; the connection closes. The message says nothing of what failed.
    AuthenticationFailed,
    /// A request before the connection authenticated, or an authentication not complete within its timeout; the
    /// connection closes.
    Unauthenticated,
    /// The connection's principal may not do what the request asks.
    PermissionDenied,
    /// The connection's credential expired before a new authentication; the connection closes.
    CredentialExpired,
}

impl ErrorCode {
    /// Every code, in the order of its number.
    pub const ALL: [ErrorCode; 22] = [
        ErrorCode::Protocol,
        ErrorCode::Conflict,
        ErrorCode::TooOld,
        ErrorCode::PastLimit,
        ErrorCode::Busy,
        ErrorCode::TooLarge,
        ErrorCode::Refused,
        ErrorCode::OperationFailed,
        ErrorCode::StaleStatement,
        ErrorCode::UnknownStatement,
        ErrorCode::Parameters,
        ErrorCode::SessionEvicted,
        ErrorCode::RequestOutOfOrder,
        ErrorCode::SessionBusy,
        ErrorCode::StreamLimit,
        ErrorCode::ProtocolVersion,
        ErrorCode::Unavailable,
        ErrorCode::NotCommitted,
        ErrorCode::AuthenticationFailed,
        ErrorCode::Unauthenticated,
        ErrorCode::PermissionDenied,
        ErrorCode::CredentialExpired,
    ];

    pub fn number(self) -> u16 {
        // The numbers of the table: ALL lists the codes in their order, from 1.
        ErrorCode::ALL
            .iter()
            .position(|c| *c == self)
            .map_or(0, |i| i as u16 + 1)
    }

    /// The code numbered `number`; `None` for a number no code has.
    pub fn from_number(number: u16) -> Option<ErrorCode> {
        ErrorCode::ALL
            .get(usize::from(number).checked_sub(1)?)
            .copied()
    }

    /// Whether a client retries the request: conflict, too old, past the limit and busy; a busy session after a delay;
    /// a request not committed by running its transaction again.
    pub fn retryable(self) -> bool {
        matches!(
            self,
            ErrorCode::Conflict
                | ErrorCode::TooOld
                | ErrorCode::PastLimit
                | ErrorCode::Busy
                | ErrorCode::SessionBusy
                | ErrorCode::NotCommitted
        )
    }
}
