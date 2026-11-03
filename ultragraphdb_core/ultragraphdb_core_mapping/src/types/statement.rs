/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

/// The canonical model's unit: a subject's predicate and object in a graph, with its reifier and properties when it has
/// them. Statements that share a graph, subject, predicate and object are one triple, told apart by their reifiers: the
/// IRIs naming their statement IDs, which no statement shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Statement {
    graph: GraphName,
    subject: NamedOrBlankNode,
    predicate: NamedNode,
    object: Term,
    reifier: Option<NamedNode>,
    properties: Vec<(NamedNode, Term)>,
}

impl Statement {
    /// The statement of `subject`'s `predicate` and `object` in `graph`, with no reifier and no property.
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
            reifier: None,
            properties: Vec::new(),
        }
    }

    /// The statement named by the reifier `reifier`.
    pub fn with_reifier(mut self, reifier: NamedNode) -> Self {
        self.reifier = Some(reifier);
        self
    }

    /// The statement with the properties `properties`.
    pub fn with_properties(mut self, properties: Vec<(NamedNode, Term)>) -> Self {
        self.properties = properties;
        self
    }

    /// The graph.
    pub fn graph(&self) -> &GraphName {
        &self.graph
    }

    /// The subject.
    pub fn subject(&self) -> &NamedOrBlankNode {
        &self.subject
    }

    /// The predicate.
    pub fn predicate(&self) -> &NamedNode {
        &self.predicate
    }

    /// The object.
    pub fn object(&self) -> &Term {
        &self.object
    }

    /// The reifier naming the statement, when it has a statement ID.
    pub fn reifier(&self) -> Option<&NamedNode> {
        self.reifier.as_ref()
    }

    /// The statement's properties, each a predicate and a value, in the order they were set.
    pub fn properties(&self) -> &[(NamedNode, Term)] {
        &self.properties
    }
}
