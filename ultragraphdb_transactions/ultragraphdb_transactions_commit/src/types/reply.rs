/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::OperationResult;

use crate::errors::commit_error::CommitError;

/// The commit step's answer to one commit request: its ticket and its commit version, or why it did not commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    ticket: u64,
    outcome: Result<u64, CommitError>,
    results: Vec<OperationResult>,
}

impl Reply {
    pub fn new(ticket: u64, outcome: Result<u64, CommitError>) -> Self {
        Self {
            ticket,
            outcome,
            results: Vec::new(),
        }
    }

    /// The reply with `results`, the commit-time operations' results in the order the transaction asked them.
    pub fn with_results(self, results: Vec<OperationResult>) -> Self {
        Self { results, ..self }
    }

    /// The commit-time operations' results, in the order the transaction asked them: empty for a transaction that
    /// asked none and for a refusal (7.5).
    pub fn results(&self) -> &[OperationResult] {
        &self.results
    }

    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    /// The commit version, or the reason.
    pub fn outcome(&self) -> &Result<u64, CommitError> {
        &self.outcome
    }
}
