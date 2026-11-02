/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use oxrdf::{NamedOrBlankNode, Term};

use crate::types::statement_id::StatementId;

/// An edge of the property-graph view: one statement with an IRI or blank-node object, its statement ID when it has
/// one, and its properties by name, each with its values ordered by their N-Triples text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgEdge {
    id: Option<StatementId>,
    source: NamedOrBlankNode,
    edge_type: String,
    target: NamedOrBlankNode,
    properties: BTreeMap<String, Vec<Term>>,
}

impl PgEdge {
    /// The edge of type `edge_type` from `source` to `target`, with no statement ID and no property.
    pub fn new(
        source: NamedOrBlankNode,
        edge_type: impl Into<String>,
        target: NamedOrBlankNode,
    ) -> Self {
        Self {
            id: None,
            source,
            edge_type: edge_type.into(),
            target,
            properties: BTreeMap::new(),
        }
    }

    /// The edge with the statement ID `id`.
    pub fn with_id(mut self, id: StatementId) -> Self {
        self.id = Some(id);
        self
    }

    /// The edge with the property `name` holding `values`, replacing any it held.
    pub fn with_property(mut self, name: impl Into<String>, mut values: Vec<Term>) -> Self {
        values.sort_by_key(|v| v.to_string());
        let _replaced: Option<Vec<Term>> = self.properties.insert(name.into(), values);
        self
    }

    /// The edge without its statement ID, as it is written before storage allocates one.
    pub fn without_id(mut self) -> Self {
        self.id = None;
        self
    }

    /// The statement ID, when it has one.
    pub fn id(&self) -> Option<StatementId> {
        self.id
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
