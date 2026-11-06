/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::refused_request::RefusedRequest;
use crate::types::runs_again::RunsAgain;

/// Why a request was refused, the Error frame's `code`. A code's number is never reused for another meaning.
///
/// | Number | Code | Retryable | `detail` |
/// |---|---|---|---|
/// | 1 | Protocol | no | —; 1 when a Scan named a transaction the partition does not hold |
/// | 2 | Conflict | yes | the [`crate::ConflictKind`] of the read that conflicted |
/// | 3 | TooOld | yes | — |
/// | 4 | PastLimit | yes | the limit, in milliseconds |
/// | 5 | Busy | yes | — |
/// | 6 | TooLarge | no | the limit, in bytes; for statement IDs, their bound |
/// | 7 | Refused | no | —; for a term's rule, the term's index; for a declaration, its record's index; for a catalog version past its cap, the cap; for an unknown tenant, the tenant; for a Lease, the maximum; for a Catalog, the version; for a schema rule at commit, the UGS refusal number |
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
/// | 23 | CrossPartition | no | the transaction's partition, bits 0–31; the lowest other partition reached, bits 32–63 |
/// | 24 | TimedOut | yes | the partition waited on |
/// | 25 | LevelRefused | no | as CrossPartition's |
/// | 26 | WrongPartition | yes, after refreshing the placement map | the bucket, bits 0–23; the owning partition, bits 32–63 |
/// | 27 | AheadOfWatermark | no | the watermark |
/// | 28 | SchemaRefused | no | the count of diagnostics |
/// | 29 | QueryRefused | no | the diagnostic's number, with the language in bits 16–23: 1 GQL, 2 SPARQL |
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
    /// A prepared statement's schema id no longer matches its tenant's catalog.
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
    /// Cross-partition work, refused in this stage: a read-write transaction reaching a second partition, or a Drop
    /// reaching several (`PROTOCOL.md` §9.4, code 23).
    CrossPartition,
    /// A read waited past its transaction's deadline for a partition to close the epoch it reads at; it returns no
    /// row (code 24).
    TimedOut,
    /// A read-only transaction at level latest whose read reaches another partition than its first read's, or more
    /// than one (code 25).
    LevelRefused,
    /// A key outside the receiving partition's bucket ranges, from a sender whose placement map is stale (code 26).
    WrongPartition,
    /// A read version the client named is above the visibility watermark of the partition answering (code 27).
    AheadOfWatermark,
    /// An Install's sources were refused: the message holds the diagnostics, and nothing was installed (code 28).
    SchemaRefused,
    /// A query or prepared text was refused: the message begins with the diagnostic's code and its place (code 29).
    QueryRefused,
}

impl ErrorCode {
    /// Every code, in the order of its number.
    pub const ALL: [ErrorCode; 29] = [
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
        ErrorCode::CrossPartition,
        ErrorCode::TimedOut,
        ErrorCode::LevelRefused,
        ErrorCode::WrongPartition,
        ErrorCode::AheadOfWatermark,
        ErrorCode::SchemaRefused,
        ErrorCode::QueryRefused,
    ];

    /// The code's number on the wire.
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

    /// Whether a client retries the request whatever kind it is: conflict, too old, past the limit and busy; a busy
    /// session after a delay; a request not committed by running its transaction again; a read timed out; a wrong
    /// partition after the placement map is refreshed. Code 17 depends on its request, so what runs again is
    /// [`ErrorCode::runs_again`]'s to say.
    pub fn retryable(self) -> bool {
        matches!(
            self,
            ErrorCode::Conflict
                | ErrorCode::TooOld
                | ErrorCode::PastLimit
                | ErrorCode::Busy
                | ErrorCode::SessionBusy
                | ErrorCode::NotCommitted
                | ErrorCode::TimedOut
                | ErrorCode::WrongPartition
        )
    }

    /// What runs again when `request`, the kind of request this code refused, is retried (`PROTOCOL.md` §9.4): `None`
    /// for a refusal running it again cannot mend.
    pub fn runs_again(self, request: RefusedRequest) -> Option<RunsAgain> {
        match self {
            ErrorCode::Conflict
            | ErrorCode::TooOld
            | ErrorCode::PastLimit
            | ErrorCode::NotCommitted
            | ErrorCode::WrongPartition => Some(RunsAgain::Transaction),
            ErrorCode::Busy => Some(match request {
                RefusedRequest::OneFrame => RunsAgain::Request,
                RefusedRequest::Read | RefusedRequest::Commit | RefusedRequest::Other => {
                    RunsAgain::Transaction
                }
            }),
            ErrorCode::SessionBusy => Some(RunsAgain::Request),
            ErrorCode::TimedOut => Some(RunsAgain::Read),
            ErrorCode::Unavailable => match request {
                RefusedRequest::Read => Some(RunsAgain::Read),
                RefusedRequest::Commit | RefusedRequest::OneFrame => Some(RunsAgain::InDoubt),
                RefusedRequest::Other => None,
            },
            ErrorCode::Protocol
            | ErrorCode::Refused
            | ErrorCode::OperationFailed
            | ErrorCode::StaleStatement
            | ErrorCode::UnknownStatement
            | ErrorCode::Parameters
            | ErrorCode::SessionEvicted
            | ErrorCode::RequestOutOfOrder
            | ErrorCode::TooLarge
            | ErrorCode::StreamLimit
            | ErrorCode::ProtocolVersion
            | ErrorCode::AuthenticationFailed
            | ErrorCode::Unauthenticated
            | ErrorCode::PermissionDenied
            | ErrorCode::CredentialExpired
            | ErrorCode::CrossPartition
            | ErrorCode::LevelRefused
            | ErrorCode::AheadOfWatermark
            | ErrorCode::SchemaRefused
            | ErrorCode::QueryRefused => None,
        }
    }
}
