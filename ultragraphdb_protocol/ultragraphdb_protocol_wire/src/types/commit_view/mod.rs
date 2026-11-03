/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod commit_view_body_view;

use ultragraphdb_core_ids::SessionToken;

use crate::types::operation_result::OperationResult;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::constants::OPERATION_RESULT_SIZE;
use crate::utils::results::result_at;

/// The body of a received Commit frame, read in place; see [`crate::Commit`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitView<'a> {
    session: [u8; 16],
    request: u64,
    commit_version: u64,
    token: Option<SessionToken>,
    results: &'a [u8],
    nodes: TermTableView<'a>,
}

impl<'a> CommitView<'a> {
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

    /// The number of results.
    pub fn result_count(&self) -> usize {
        self.results.len() / OPERATION_RESULT_SIZE
    }

    /// Result `index`, `None` past the last.
    pub fn result(&self, index: usize) -> Option<OperationResult> {
        result_at(self.results, index)
    }

    /// The IRI or skolem IRI of the node result `index` names, borrowed from the frame; `None` for a result naming
    /// none or past the last.
    pub fn node(&self, index: usize) -> Option<&'a str> {
        let node = self.result(index)?.node()?;
        if let RequestTermView::Iri(iri) = self.nodes.get(node)? {
            return Some(iri);
        }
        None
    }

    /// The reply's term table of the results' nodes.
    pub fn nodes(&self) -> TermTableView<'a> {
        self.nodes
    }
}
