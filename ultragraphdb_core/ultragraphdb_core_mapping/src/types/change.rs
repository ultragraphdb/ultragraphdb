/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

use crate::types::declaration::Declaration;
use crate::types::statement_ref::StatementRef;

/// One operation of a [`crate::Changeset`], in RDF terms.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Change {
    /// The triple, held once whatever how often it is written, as RDF holds statements; for a single-valued
    /// predicate it replaces the subject's object.
    Insert {
        /// The graph.
        graph: GraphName,
        /// The subject.
        subject: NamedOrBlankNode,
        /// The predicate.
        predicate: NamedNode,
        /// The object.
        object: Term,
    },
    /// A statement of its own, beside any that hold the same triple, as a property graph creates an edge: a parallel
    /// edge, or an edge with properties, takes a statement ID.
    InsertEdge {
        /// The graph.
        graph: GraphName,
        /// The subject.
        subject: NamedOrBlankNode,
        /// The predicate.
        predicate: NamedNode,
        /// The object.
        object: Term,
        /// The statement's properties.
        properties: Vec<(NamedNode, Term)>,
    },
    /// The triple and every statement that holds it.
    Delete {
        /// The graph.
        graph: GraphName,
        /// The subject.
        subject: NamedOrBlankNode,
        /// The predicate.
        predicate: NamedNode,
        /// The object.
        object: Term,
    },
    /// One statement, by its reifier.
    DeleteStatement {
        /// The graph.
        graph: GraphName,
        /// The reifier naming the statement's ID.
        reifier: NamedNode,
    },
    /// Every object of the subject's predicate in the graph.
    DeletePattern {
        /// The graph.
        graph: GraphName,
        /// The subject.
        subject: NamedOrBlankNode,
        /// The predicate.
        predicate: NamedNode,
    },
    /// Adds a property to a statement, which takes a statement ID if it has none.
    SetProperty {
        /// The statement.
        statement: StatementRef,
        /// The predicate.
        predicate: NamedNode,
        /// The property's value.
        value: Term,
    },
    /// Removes a statement's property, one value of it or, with no value, every value.
    DeleteProperty {
        /// The statement.
        statement: StatementRef,
        /// The predicate.
        predicate: NamedNode,
        /// The value removed, or none for every value.
        value: Option<Term>,
    },
    /// A catalog declaration.
    Declare(Declaration),
}
