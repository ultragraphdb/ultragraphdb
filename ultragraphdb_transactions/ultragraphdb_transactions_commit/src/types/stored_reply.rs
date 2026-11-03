/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_protocol_wire::{OperationResult, TermTable};

/// A session's last committed request's whole reply in a partition, as the session table keeps it: the commit version,
/// the session token and every commit-time operation's result with the reply's term table, which names each node by
/// its IRI or skolem IRI, so a retry is answered as the original was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredReply {
    version: u64,
    token: SessionToken,
    results: Vec<OperationResult>,
    terms: TermTable,
}

impl StoredReply {
    /// The reply of a commit at `version` answering `token`, with `results` naming their nodes in `terms`.
    pub fn new(
        version: u64,
        token: SessionToken,
        results: Vec<OperationResult>,
        terms: TermTable,
    ) -> Self {
        Self {
            version,
            token,
            results,
            terms,
        }
    }

    /// The commit version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The session token.
    pub fn token(&self) -> &SessionToken {
        &self.token
    }

    /// The commit-time operations' results, in the order the transaction asked them.
    pub fn results(&self) -> &[OperationResult] {
        &self.results
    }

    /// The reply's term table: the nodes the results name, by their IRI or skolem IRI.
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }
}
