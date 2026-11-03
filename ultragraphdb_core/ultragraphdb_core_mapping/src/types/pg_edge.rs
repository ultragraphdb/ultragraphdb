/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use oxrdf::{NamedNode, NamedOrBlankNode, Term};

/// An edge of the property-graph view: one statement with an IRI or blank-node object, the reifier naming its
/// statement ID when it has one, and its properties by name, each with its values ordered by their N-Triples text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgEdge {
    reifier: Option<NamedNode>,
    source: NamedOrBlankNode,
    edge_type: String,
    target: NamedOrBlankNode,
    properties: BTreeMap<String, Vec<Term>>,
}

impl PgEdge {
    /// The edge of type `edge_type` from `source` to `target`, with no reifier and no property.
    pub fn new(
        source: NamedOrBlankNode,
        edge_type: impl Into<String>,
        target: NamedOrBlankNode,
    ) -> Self {
        Self {
            reifier: None,
            source,
            edge_type: edge_type.into(),
            target,
            properties: BTreeMap::new(),
        }
    }

    /// The edge named by the reifier `reifier`.
    pub fn with_reifier(mut self, reifier: NamedNode) -> Self {
        self.reifier = Some(reifier);
        self
    }

    /// The edge with the property `name` holding `values`, replacing any it held.
    pub fn with_property(mut self, name: impl Into<String>, mut values: Vec<Term>) -> Self {
        values.sort_by_key(|v| v.to_string());
        let _replaced: Option<Vec<Term>> = self.properties.insert(name.into(), values);
        self
    }

    /// The edge without its reifier, as it is written before storage allocates its statement ID.
    pub fn without_reifier(mut self) -> Self {
        self.reifier = None;
        self
    }

    /// The reifier, when the edge has a statement ID.
    pub fn reifier(&self) -> Option<&NamedNode> {
        self.reifier.as_ref()
    }

    /// The source node.
    pub fn source(&self) -> &NamedOrBlankNode {
        &self.source
    }

    /// The edge's type.
    pub fn edge_type(&self) -> &str {
        &self.edge_type
    }

    /// The target node.
    pub fn target(&self) -> &NamedOrBlankNode {
        &self.target
    }

    /// The properties by name.
    pub fn properties(&self) -> &BTreeMap<String, Vec<Term>> {
        &self.properties
    }
}
