/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::commit_error::CommitError;

/// The commit step's answer to one commit request: its ticket and its commit version, or why it did not commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    ticket: u64,
    outcome: Result<u64, CommitError>,
}

impl Reply {
    pub fn new(ticket: u64, outcome: Result<u64, CommitError>) -> Self {
        Self { ticket, outcome }
    }

    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    /// The commit version, or the reason.
    pub fn outcome(&self) -> &Result<u64, CommitError> {
        &self.outcome
    }
}
