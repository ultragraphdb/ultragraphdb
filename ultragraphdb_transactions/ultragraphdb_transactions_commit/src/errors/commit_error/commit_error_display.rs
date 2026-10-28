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
                "the write set takes a round of {size} bytes, over the round bound of {limit}"
            ),
            CommitErrorEnum::Malformed { cause } => {
                write!(f, "a write-set record is malformed: {cause}")
            }
            CommitErrorEnum::Unsealed => f.write_str("a read set reached the commit step unsealed"),
            CommitErrorEnum::Unavailable => {
                f.write_str("the node refuses writes after a failed log sync, until it restarts")
            }
        }
    }
}
