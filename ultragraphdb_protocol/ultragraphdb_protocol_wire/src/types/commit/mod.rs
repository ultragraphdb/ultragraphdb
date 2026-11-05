/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod commit_body_builder;

use ultragraphdb_core_ids::SessionToken;

use crate::types::operation_result::OperationResult;
use crate::types::request_term::RequestTerm;
use crate::types::term_table::TermTable;

/// A transaction commits, through its session with a request number or through no session (`PROTOCOL.md` §6.3): sent
/// with `commit_version` zero, no token, no results and an empty term table; answered with its commit version, the
/// commit's session token and the result of each commit-time operation it asked for, in the order asked, each node
/// named by its IRI or skolem IRI through the reply's term table, never by a system ID.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `session` | 16 bytes; all zero for none |
/// | 16 | `request` | u64; at least 1 with a session, 0 with none |
/// | 24 | `commit_version` | u64 |
/// | 32 | `token` | session token, 24 bytes; zero in a request |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | results, 16 bytes each ([`OperationResult`]) |
/// | 1 | the term table's records: the results' nodes, IRI entries interned in result order |
/// | 2 | the term table's text |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    session: [u8; 16],
    request: u64,
    commit_version: u64,
    token: Option<SessionToken>,
    results: Vec<OperationResult>,
    nodes: TermTable,
}

impl Commit {
    /// A commit of `session`'s request `request`: the request when `commit_version` is zero, else the reply, whose
    /// token and results [`Commit::with_token`], [`Commit::push_result`] and [`Commit::push_node_result`] add.
    pub fn new(session: [u8; 16], request: u64, commit_version: u64) -> Self {
        Self {
            session,
            request,
            commit_version,
            token: None,
            results: Vec::new(),
            nodes: TermTable::new(),
        }
    }

    /// The reply carrying the commit's session token.
    pub fn with_token(mut self, token: SessionToken) -> Self {
        self.token = Some(token);
        self
    }

    /// Appends the result of an insert-if-absent or upsert-and-link that found or created the node of IRI or skolem
    /// IRI `node`, interned once in the reply's term table.
    pub fn push_node_result(&mut self, node: &str, created: bool) {
        let index = self.nodes.iri(node);
        self.results.push(OperationResult::for_node(index, created));
    }

    /// Appends a result naming no node: a counter add's or a compare-and-set's.
    pub fn push_result(&mut self, result: OperationResult) {
        self.results.push(result);
    }

    /// The client's session; all zero for none.
    pub fn session(&self) -> [u8; 16] {
        self.session
    }

    /// The request's number within its session.
    pub fn request(&self) -> u64 {
        self.request
    }

    /// The commit version; zero in a request.
    pub fn commit_version(&self) -> u64 {
        self.commit_version
    }

    /// The commit's session token; `None` in a request.
    pub fn token(&self) -> Option<&SessionToken> {
        self.token.as_ref()
    }

    /// The commit-time operations' results, in the order asked.
    pub fn results(&self) -> &[OperationResult] {
        &self.results
    }

    /// The IRI or skolem IRI of the node result `index` names; `None` for a result naming none or past the last.
    pub fn node(&self, index: usize) -> Option<&str> {
        let node = self.results.get(index)?.node()?;
        if let RequestTerm::Iri(iri) = self.nodes.get(node)? {
            return Some(iri.as_str());
        }
        None
    }

    pub(crate) fn nodes(&self) -> &TermTable {
        &self.nodes
    }

    pub(crate) fn set_nodes(&mut self, nodes: TermTable, results: Vec<OperationResult>) {
        self.nodes = nodes;
        self.results = results;
    }
}
