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
/// | 7 | Refused | no | — |
/// | 8 | OperationFailed | no | — (the value found is the frame's value) |
/// | 9 | StaleStatement | no | the statement |
/// | 10 | UnknownStatement | no | the statement |
/// | 11 | Parameters | no | the statement |
/// | 12 | SessionEvicted | no | the session |
/// | 13 | RequestOutOfOrder | no | the request number expected |
/// | 14 | SessionBusy | no | the session |
/// | 15 | StreamLimit | no | the limit |
/// | 16 | ProtocolVersion | no | the server's highest protocol |
/// | 17 | Unavailable | no | — |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ErrorCode {
    /// The frame broke a validation rule; the connection closes after this reply.
    Protocol,
    /// A transaction's read conflicted with a later-ordered commit.
    Conflict,
    /// A transaction's read version is below what validation still holds.
    TooOld,
    /// A transaction lived past its limit.
    PastLimit,
    /// The commit queue is full.
    Busy,
    /// A write set exceeds the maximum frame.
    TooLarge,
    /// The catalog refused a write.
    Refused,
    /// A compare-and-set did not hold.
    OperationFailed,
    /// A prepared statement's schema id no longer matches its namespace's catalog.
    StaleStatement,
    /// No statement of the session has the id executed.
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
}

impl ErrorCode {
    /// Every code, in the order of its number.
    pub const ALL: [ErrorCode; 17] = [
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

    /// Whether a client retries the request: conflict, too old, past the limit and busy.
    pub fn retryable(self) -> bool {
        matches!(
            self,
            ErrorCode::Conflict | ErrorCode::TooOld | ErrorCode::PastLimit | ErrorCode::Busy
        )
    }
}
