/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_protocol_wire::{Commit, OperationResult, RequestTerm, TermTable};

/// A commit's reply, whether it answers the Commit or a later Recall of it: request `request` of `session` at `version`, its
/// `token`, and its operations' `results`, each node by its text.
pub(crate) fn commit_body(
    (session, request): (u128, u64),
    version: u64,
    token: Option<&SessionToken>,
    results: &[OperationResult],
    terms: &TermTable,
) -> Commit {
    let mut body = Commit::new(session.to_be_bytes(), request, version);
    if let Some(token) = token {
        body = body.with_token(token.clone());
    }
    for result in results {
        match result.node().and_then(|index| terms.get(index)) {
            Some(RequestTerm::Iri(node)) => body.push_node_result(node, result.created()),
            _ => body.push_result(*result),
        }
    }
    body
}
