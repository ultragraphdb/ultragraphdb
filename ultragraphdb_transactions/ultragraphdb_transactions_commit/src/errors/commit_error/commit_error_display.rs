/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::commit_error::{CommitError, CommitErrorEnum};

impl fmt::Display for CommitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            // Dgraph's texts: QuadError::Aborted's and posting's ErrTsTooOld.
            CommitErrorEnum::Conflict { .. } => {
                f.write_str("Transaction has been aborted. Please retry")
            }
            CommitErrorEnum::TooOld { .. } => f.write_str("Transaction is too old"),
            CommitErrorEnum::TooLarge { size, limit } => write!(
                f,
                "the write-set record of {size} bytes is over the largest a round holds, {limit} bytes"
            ),
            CommitErrorEnum::Malformed { cause } => {
                write!(f, "a write-set record is malformed: {cause}")
            }
            CommitErrorEnum::Unsealed => f.write_str("a read set reached the commit step unsealed"),
            CommitErrorEnum::Unavailable => {
                f.write_str("the node refuses writes after a failed log sync, until it restarts")
            }
            CommitErrorEnum::Busy { limit } => {
                write!(f, "the commit queue is full at {limit} requests: retry")
            }
            CommitErrorEnum::SessionEvicted { session } => {
                write!(
                    f,
                    "session {session} was evicted from the session table: register a new one"
                )
            }
            CommitErrorEnum::RequestOutOfOrder { expected } => {
                write!(
                    f,
                    "the request number is out of order: {expected} is expected"
                )
            }
            CommitErrorEnum::SessionBusy { session } => {
                write!(f, "session {session} already has a commit in flight")
            }
            CommitErrorEnum::NotCommitted { request } => {
                write!(
                    f,
                    "request {request} was not committed: run its transaction again"
                )
            }
            CommitErrorEnum::PermissionDenied { session } => {
                write!(f, "session {session} belongs to another principal")
            }
            CommitErrorEnum::OperationFailed { found: Some(found) } => {
                write!(f, "the compare-and-set did not hold: {found} is held")
            }
            CommitErrorEnum::OperationFailed { found: None } => {
                f.write_str("the compare-and-set did not hold: no value is held")
            }
            CommitErrorEnum::Refused { reason } => {
                write!(f, "the commit-time operation was refused: {reason}")
            }
        }
    }
}
