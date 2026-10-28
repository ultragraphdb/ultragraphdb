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
    Conflict { kind: ConflictKind },
    /// The read version is below `floor`, the version the conflict history is pruned to. Retryable.
    TooOld { read_version: u64, floor: u64 },
    /// The transaction's write-set record cannot fit a round: a round of it alone is `size` bytes, over the round's
    /// bound of `limit`.
    TooLarge { size: usize, limit: usize },
    /// The write-set record does not read as one: a worker built it wrongly.
    Malformed { cause: ProtocolError },
    /// The read set was handed over unsealed, so its tables are not in the order validation walks them.
    Unsealed,
    /// The node refuses writes, after a failed log sync or once commit versions are exhausted, until it restarts.
    Unavailable,
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

    /// The Error frame's code, which the Round decision records as its outcome.
    pub fn code(&self) -> ErrorCode {
        match self.0 {
            CommitErrorEnum::Conflict { .. } => ErrorCode::Conflict,
            CommitErrorEnum::TooOld { .. } => ErrorCode::TooOld,
            CommitErrorEnum::TooLarge { .. } => ErrorCode::TooLarge,
            CommitErrorEnum::Malformed { .. } | CommitErrorEnum::Unsealed => ErrorCode::Protocol,
            CommitErrorEnum::Unavailable => ErrorCode::Unavailable,
        }
    }

    /// The code's detail: the conflict's kind, or the too-large limit in bytes; zero otherwise.
    pub fn detail(&self) -> u64 {
        match self.0 {
            CommitErrorEnum::Conflict { kind } => kind.number(),
            CommitErrorEnum::TooLarge { limit, .. } => limit as u64,
            _ => 0,
        }
    }

    /// Whether a retry of the transaction may commit: a conflict and too old are.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.0,
            CommitErrorEnum::Conflict { .. } | CommitErrorEnum::TooOld { .. }
        )
    }
}
