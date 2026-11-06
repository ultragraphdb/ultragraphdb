/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_protocol_wire::OperationResult;

/// A commit's answer: its commit version, its session token, and the result of each commit-time operation, in the order
/// asked, with the nodes they name; the request number of its session it committed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Committed {
    version: u64,
    request: u64,
    results: Vec<OperationResult>,
    token: Option<SessionToken>,
    nodes: Vec<Option<String>>,
}

impl Committed {
    pub(crate) fn new(version: u64, request: u64) -> Self {
        Self {
            version,
            request,
            results: Vec::new(),
            token: None,
            nodes: Vec::new(),
        }
    }

    /// The answer with its session token, a reply through no session holding none.
    pub(crate) fn with_token(mut self, token: Option<SessionToken>) -> Self {
        self.token = token;
        self
    }

    /// The answer with its operations' `results` and the node each names.
    pub(crate) fn with_results(
        mut self,
        results: Vec<OperationResult>,
        nodes: Vec<Option<String>>,
    ) -> Self {
        self.results = results;
        self.nodes = nodes;
        self
    }

    /// The commit version.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The request number of the session that committed.
    #[must_use]
    pub fn request(&self) -> u64 {
        self.request
    }

    /// The commit's session token: its partition, its round's position and its epoch; `None` through no session.
    #[must_use]
    pub fn token(&self) -> Option<&SessionToken> {
        self.token.as_ref()
    }

    /// The IRI or skolem IRI of the node result `index` names; `None` for a result naming none or past the last.
    #[must_use]
    pub fn node(&self, index: usize) -> Option<&str> {
        self.nodes.get(index)?.as_deref()
    }

    /// The results of the commit-time operations, in the order asked.
    #[must_use]
    pub fn results(&self) -> &[OperationResult] {
        &self.results
    }
}
