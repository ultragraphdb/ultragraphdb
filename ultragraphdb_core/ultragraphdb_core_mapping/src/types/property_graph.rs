/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use crate::types::pg_edge::PgEdge;
use crate::types::pg_node::PgNode;

/// One graph's statements as a property graph: its nodes by their N-Triples text, and its edges ordered by source,
/// type, target and statement ID.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PropertyGraph {
    nodes: BTreeMap<String, PgNode>,
    edges: Vec<PgEdge>,
}

impl PropertyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// The graph with `node`, replacing a node of the same identity.
    pub fn with_node(mut self, node: PgNode) -> Self {
        self.nodes.insert(node.id().to_string(), node);
        self
    }

    pub fn with_edge(mut self, edge: PgEdge) -> Self {
        self.edges.push(edge);
        self.sort_edges();
        self
    }

    pub fn nodes(&self) -> impl Iterator<Item = &PgNode> {
        self.nodes.values()
    }

    pub fn node(&self, id: &str) -> Option<&PgNode> {
        self.nodes.get(id)
    }

    pub fn edges(&self) -> &[PgEdge] {
        &self.edges
    }

    /// The graph with every edge's statement ID removed, to compare a graph read back with the one written.
    pub fn without_edge_ids(mut self) -> Self {
        self.edges = self.edges.into_iter().map(PgEdge::without_id).collect();
        self.sort_edges();
        self
    }

    pub(crate) fn node_mut(&mut self, node: PgNode) -> &mut PgNode {
        self.nodes.entry(node.id().to_string()).or_insert(node)
    }

    pub(crate) fn push_edge(&mut self, edge: PgEdge) {
        self.edges.push(edge);
    }

    pub(crate) fn sort_edges(&mut self) {
        self.edges.sort_by_key(|e| {
            (
                e.source().to_string(),
                e.edge_type().to_string(),
                e.target().to_string(),
                e.id(),
                format!("{:?}", e.properties()),
            )
        });
    }
}
