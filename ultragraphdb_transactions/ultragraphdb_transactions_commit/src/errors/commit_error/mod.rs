/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod commit_error_display;
mod commit_error_error;

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{ConflictKind, ErrorCode};

/// Why the commit step did not commit a transaction: the reason its reply names and its Round decision records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitError(pub CommitErrorEnum);

/// The reasons, each with what its reply names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitErrorEnum {
    /// A transaction committed above the read version wrote what this one read: `kind` names the kind of read, or
    /// [`ConflictKind::Drop`] when the write was a range. Retryable.
    Conflict {
        /// The kind of read the write overlapped.
        kind: ConflictKind,
    },
    /// The read version is below `floor`, the version the conflict history is pruned to. Retryable.
    TooOld {
        /// The transaction's read version.
        read_version: u64,
        /// The version the conflict history is pruned to.
        floor: u64,
    },
    /// The transaction's write-set record, `size` bytes as it arrived or as its commit-time operations left it, is over
    /// `limit`, the largest a round holds on its own ([`crate::RoundBounds::max_write_set`]), the Welcome's
    /// `max_write_set`.
    TooLarge {
        /// The record's size in bytes.
        size: usize,
        /// The largest record a round holds, in bytes.
        limit: usize,
    },
    /// The write-set record does not read as one: a worker built it wrongly.
    Malformed {
        /// Why the record did not read.
        cause: ProtocolError,
    },
    /// The read set was handed over unsealed, so its tables are not in the order validation walks them.
    Unsealed,
    /// The commit queue holds `limit` requests already. Retryable.
    Busy {
        /// The commit queue's capacity, in requests.
        limit: usize,
    },
    /// The partition refuses writes, after a failed log sync, once commit versions are exhausted, or after a close
    /// that broke the order of closes, until it restarts.
    Unavailable,
    /// A close of epoch `close` was delivered while the latest close the partition holds is `delivered`: closes arrive
    /// one above the last, so the partition stops, as for an invariant violation, and answers unavailable.
    CloseOutOfOrder {
        /// The latest close the partition holds.
        delivered: u64,
        /// The close delivered.
        close: u64,
    },
    /// The commit's request number is below `last`, its session's last committed request in the partition.
    RequestOutOfOrder {
        /// The session's last committed request in the partition.
        last: u64,
    },
    /// The session already has a commit in its partition's commit step, decided and not yet applied or waiting to be
    /// decided; its reply comes first. Retryable after a delay.
    SessionBusy {
        /// The session with a commit in the commit step.
        session: u128,
    },
    /// A retry of `request`, with no transaction open for it, that the session table does not hold: the request was not
    /// committed, and the client runs its transaction again. Retryable.
    NotCommitted {
        /// The request number retried.
        request: u64,
    },
    /// A commit-time compare-and-set whose expected value was not held, or a counter add that could not add: the whole
    /// transaction is refused and wrote nothing; `found` is the value held, an N-Triples term, `None` when none is.
    OperationFailed {
        /// The value held, an N-Triples term; `None` when none is.
        found: Option<String>,
    },
    /// The catalog or the store refused a commit-time operation's write, as `reason` states; the transaction wrote
    /// nothing.
    Refused {
        /// Why the catalog or the store refused the write.
        reason: String,
    },
    /// The transaction, bound to `partition`, reached `other`'s state: an insert-if-absent or upsert-and-link claiming
    /// a value whose bucket `other` owns. Cross-partition work is refused in this stage; the transaction wrote nothing.
    CrossPartition {
        /// The partition the transaction is bound to.
        partition: u32,
        /// The other partition it reached.
        other: u32,
    },
}

impl CommitError {
    /// The [`CommitErrorEnum::Conflict`] reason.
    pub fn conflict(kind: ConflictKind) -> Self {
        Self(CommitErrorEnum::Conflict { kind })
    }

    /// The [`CommitErrorEnum::TooOld`] reason.
    pub fn too_old(read_version: u64, floor: u64) -> Self {
        Self(CommitErrorEnum::TooOld {
            read_version,
            floor,
        })
    }

    /// The [`CommitErrorEnum::TooLarge`] reason.
    pub fn too_large(size: usize, limit: usize) -> Self {
        Self(CommitErrorEnum::TooLarge { size, limit })
    }

    /// The [`CommitErrorEnum::Malformed`] reason.
    pub fn malformed(cause: ProtocolError) -> Self {
        Self(CommitErrorEnum::Malformed { cause })
    }

    /// The [`CommitErrorEnum::Unsealed`] reason.
    pub fn unsealed() -> Self {
        Self(CommitErrorEnum::Unsealed)
    }

    /// The [`CommitErrorEnum::Unavailable`] reason.
    pub fn unavailable() -> Self {
        Self(CommitErrorEnum::Unavailable)
    }

    /// The [`CommitErrorEnum::Busy`] reason.
    pub fn busy(limit: usize) -> Self {
        Self(CommitErrorEnum::Busy { limit })
    }

    /// The [`CommitErrorEnum::CloseOutOfOrder`] reason.
    pub fn close_out_of_order(delivered: u64, close: u64) -> Self {
        Self(CommitErrorEnum::CloseOutOfOrder { delivered, close })
    }

    /// The [`CommitErrorEnum::RequestOutOfOrder`] reason.
    pub fn request_out_of_order(last: u64) -> Self {
        Self(CommitErrorEnum::RequestOutOfOrder { last })
    }

    /// The [`CommitErrorEnum::SessionBusy`] reason.
    pub fn session_busy(session: u128) -> Self {
        Self(CommitErrorEnum::SessionBusy { session })
    }

    /// The [`CommitErrorEnum::NotCommitted`] reason.
    pub fn not_committed(request: u64) -> Self {
        Self(CommitErrorEnum::NotCommitted { request })
    }

    /// The [`CommitErrorEnum::OperationFailed`] reason.
    pub fn operation_failed(found: Option<String>) -> Self {
        Self(CommitErrorEnum::OperationFailed { found })
    }

    /// The [`CommitErrorEnum::Refused`] reason.
    pub fn refused(reason: String) -> Self {
        Self(CommitErrorEnum::Refused { reason })
    }

    /// The [`CommitErrorEnum::CrossPartition`] reason.
    pub fn cross_partition(partition: u32, other: u32) -> Self {
        Self(CommitErrorEnum::CrossPartition { partition, other })
    }

    /// The value a failed compare-and-set found, the Error frame's value: an N-Triples term, `None` when the failure
    /// is another or no value was held.
    pub fn found(&self) -> Option<&str> {
        match &self.0 {
            CommitErrorEnum::OperationFailed { found } => found.as_deref(),
            CommitErrorEnum::Conflict { .. }
            | CommitErrorEnum::TooOld { .. }
            | CommitErrorEnum::TooLarge { .. }
            | CommitErrorEnum::Malformed { .. }
            | CommitErrorEnum::Unsealed
            | CommitErrorEnum::Busy { .. }
            | CommitErrorEnum::Unavailable
            | CommitErrorEnum::CloseOutOfOrder { .. }
            | CommitErrorEnum::RequestOutOfOrder { .. }
            | CommitErrorEnum::SessionBusy { .. }
            | CommitErrorEnum::NotCommitted { .. }
            | CommitErrorEnum::Refused { .. }
            | CommitErrorEnum::CrossPartition { .. } => None,
        }
    }

    /// The Error frame's code, which the Round decision records as its outcome.
    pub fn code(&self) -> ErrorCode {
        match self.0 {
            CommitErrorEnum::Conflict { .. } => ErrorCode::Conflict,
            CommitErrorEnum::TooOld { .. } => ErrorCode::TooOld,
            CommitErrorEnum::TooLarge { .. } => ErrorCode::TooLarge,
            CommitErrorEnum::Malformed { .. } | CommitErrorEnum::Unsealed => ErrorCode::Protocol,
            CommitErrorEnum::Unavailable | CommitErrorEnum::CloseOutOfOrder { .. } => {
                ErrorCode::Unavailable
            }
            CommitErrorEnum::Busy { .. } => ErrorCode::Busy,
            CommitErrorEnum::RequestOutOfOrder { .. } => ErrorCode::RequestOutOfOrder,
            CommitErrorEnum::SessionBusy { .. } => ErrorCode::SessionBusy,
            CommitErrorEnum::NotCommitted { .. } => ErrorCode::NotCommitted,
            CommitErrorEnum::OperationFailed { .. } => ErrorCode::OperationFailed,
            CommitErrorEnum::Refused { .. } => ErrorCode::Refused,
            CommitErrorEnum::CrossPartition { .. } => ErrorCode::CrossPartition,
        }
    }

    /// The code's detail: the conflict's kind, the too-large limit in bytes, the last request number the partition holds
    /// for the session, the request number not committed, the two partitions of a cross-partition abort (the
    /// transaction's in bits 0–31, the other in bits 32–63); zero otherwise. The too-large limit is the largest record a
    /// round holds, the Welcome's `max_write_set`.
    pub fn detail(&self) -> u64 {
        match self.0 {
            CommitErrorEnum::Conflict { kind } => kind.number(),
            CommitErrorEnum::TooLarge { limit, .. } => limit as u64,
            CommitErrorEnum::RequestOutOfOrder { last } => last,
            CommitErrorEnum::NotCommitted { request } => request,
            CommitErrorEnum::CrossPartition { partition, other } => {
                u64::from(partition) | (u64::from(other) << 32)
            }
            CommitErrorEnum::TooOld { .. }
            | CommitErrorEnum::Malformed { .. }
            | CommitErrorEnum::Unsealed
            | CommitErrorEnum::Busy { .. }
            | CommitErrorEnum::Unavailable
            | CommitErrorEnum::CloseOutOfOrder { .. }
            | CommitErrorEnum::SessionBusy { .. }
            | CommitErrorEnum::OperationFailed { .. }
            | CommitErrorEnum::Refused { .. } => 0,
        }
    }

    /// Whether a retry of the transaction may commit: a conflict, too old, busy, session busy (after a delay) and not
    /// committed are, as `PROTOCOL.md` §9.4 marks their codes; cross-partition is not, since the transaction run again
    /// reaches the same partitions.
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
