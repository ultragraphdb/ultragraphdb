/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

use crate::types::statement_id::StatementId;

/// The canonical model's unit: a subject's predicate and object in a graph, with its statement ID and properties when
/// it has them. Statements that share a graph, subject, predicate and object are one triple, told apart by their IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Statement {
    graph: GraphName,
    subject: NamedOrBlankNode,
    predicate: NamedNode,
    object: Term,
    id: Option<StatementId>,
    properties: Vec<(NamedNode, Term)>,
}

impl Statement {
    pub fn new(
        graph: GraphName,
        subject: NamedOrBlankNode,
        predicate: NamedNode,
        object: Term,
    ) -> Self {
        Self {
            graph,
            subject,
            predicate,
            object,
            id: None,
            properties: Vec::new(),
        }
    }

    pub fn with_id(mut self, id: StatementId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn with_properties(mut self, properties: Vec<(NamedNode, Term)>) -> Self {
        self.properties = properties;
        self
    }

    pub fn graph(&self) -> &GraphName {
        &self.graph
    }

    pub fn subject(&self) -> &NamedOrBlankNode {
        &self.subject
    }

    pub fn predicate(&self) -> &NamedNode {
        &self.predicate
    }

    pub fn object(&self) -> &Term {
        &self.object
    }

    pub fn id(&self) -> Option<StatementId> {
        self.id
    }

    /// The statement's properties, each a predicate and a value, in the order they were set.
    pub fn properties(&self) -> &[(NamedNode, Term)] {
        &self.properties
    }
}
