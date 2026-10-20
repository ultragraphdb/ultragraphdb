/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb;

use crate::types::graph_query::GraphQuery;
use crate::types::vars::Vars;

/// What a query parses into, Dgraph's `dql.Result`: its blocks, the variables each defines and uses, and a
/// schema request.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParseResult {
    pub(crate) query: Vec<GraphQuery>,
    pub(crate) query_vars: Vec<Vars>,
    pub(crate) schema: Option<pb::SchemaRequest>,
}

impl ParseResult {
    /// The query blocks, in order.
    pub fn query(&self) -> &[GraphQuery] {
        &self.query
    }

    /// The variables of each query block, by position.
    pub fn query_vars(&self) -> &[Vars] {
        &self.query_vars
    }

    /// The `schema {}` block.
    pub fn schema(&self) -> Option<&pb::SchemaRequest> {
        self.schema.as_ref()
    }
}
