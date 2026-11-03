/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::commit_error::{CommitError, CommitErrorEnum};

impl fmt::Display for CommitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            // Dgraph's texts: QuadError::aborted's and posting's ErrTsTooOld.
            CommitErrorEnum::Conflict { .. } => f.write_str("Transaction has been aborted. Please retry"),
            CommitErrorEnum::TooOld { .. } => f.write_str("Transaction is too old"),
            CommitErrorEnum::TooLarge { size, limit } => write!(
                f,
                "the write-set record of {size} bytes is over the largest a round holds, {limit} bytes"
            ),
            CommitErrorEnum::Malformed { cause } => write!(f, "a write-set record is malformed: {cause}"),
            CommitErrorEnum::Unsealed => f.write_str("a read set reached the commit step unsealed"),
            CommitErrorEnum::Unavailable => f.write_str(
                "the partition refuses writes after a failed log sync, exhausted versions or a close out of order, \
                 until it restarts",
            ),
            CommitErrorEnum::CloseOutOfOrder { delivered, close } => write!(
                f,
                "the close of epoch {close} was delivered after the close of epoch {delivered}: closes arrive one \
                 above the last, so the partition stops"
            ),
            CommitErrorEnum::Busy { limit } => write!(
                f,
                "the commit queue is full at {limit} requests, or the partition waits for a close it asked for: retry"
            ),
            CommitErrorEnum::RequestOutOfOrder { last } => write!(
                f,
                "the request number is below the session's last committed request in this partition, {last}"
            ),
            CommitErrorEnum::SessionBusy { session } => {
                write!(f, "session {session:032x} already has a commit in the commit step")
            }
            CommitErrorEnum::NotCommitted { request } => {
                write!(f, "request {request} was not committed: run its transaction again")
            }
            CommitErrorEnum::OperationFailed { found: Some(found) } => {
                write!(f, "the compare-and-set did not hold: {found} is held")
            }
            CommitErrorEnum::OperationFailed { found: None } => {
                f.write_str("the compare-and-set did not hold: no value is held")
            }
            CommitErrorEnum::Refused { reason } => write!(f, "the commit-time operation was refused: {reason}"),
            CommitErrorEnum::CrossPartition { partition, other } => write!(
                f,
                "the transaction, bound to partition {partition}, reached partition {other}: cross-partition work is \
                 refused in this stage"
            ),
        }
    }
}
