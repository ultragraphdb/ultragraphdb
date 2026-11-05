/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_protocol_wire::{OperationResult, TermTable};

use crate::errors::commit_error::CommitError;

/// The commit step's answer to one commit request: its ticket and its commit version, or why it did not commit; the
/// session token of a commit through a session, naming its partition, its round's log position and its version's epoch;
/// and its commit-time operations' results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    ticket: u64,
    outcome: Result<u64, CommitError>,
    token: Option<SessionToken>,
    results: Vec<OperationResult>,
    terms: TermTable,
}

impl Reply {
    /// The reply to `ticket` with `outcome` and no operation results.
    pub fn new(ticket: u64, outcome: Result<u64, CommitError>) -> Self {
        Self {
            ticket,
            outcome,
            token: None,
            results: Vec::new(),
            terms: TermTable::new(),
        }
    }

    /// The reply with `token`, a commit's session token.
    pub fn with_token(self, token: SessionToken) -> Self {
        Self {
            token: Some(token),
            ..self
        }
    }

    /// The session token of a commit through a session: its partition, its round's log position and its version's
    /// epoch; `None` for a commit through no session and for a refusal.
    pub fn token(&self) -> Option<&SessionToken> {
        self.token.as_ref()
    }

    /// The reply with `results`, the commit-time operations' results in the order the transaction asked them, naming
    /// their nodes in `terms`.
    pub fn with_results(self, results: Vec<OperationResult>, terms: TermTable) -> Self {
        Self {
            results,
            terms,
            ..self
        }
    }

    /// The reply's term table: the nodes the results name, by their IRI or skolem IRI.
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    /// The commit-time operations' results, in the order the transaction asked them: empty for a transaction that
    /// asked none and for a refusal (7.5).
    pub fn results(&self) -> &[OperationResult] {
        &self.results
    }

    /// The ticket of the request answered.
    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    /// The commit version, or the reason.
    pub fn outcome(&self) -> &Result<u64, CommitError> {
        &self.outcome
    }
}
