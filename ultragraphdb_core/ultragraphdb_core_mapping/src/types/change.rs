/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

use crate::types::declaration::Declaration;
use crate::types::statement_id::StatementId;
use crate::types::statement_ref::StatementRef;

/// One operation of a [`crate::Changeset`], in RDF terms.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Change {
    /// The triple, held once whatever how often it is written, as RDF holds statements; for a single-valued
    /// predicate it replaces the subject's object.
    Insert {
        graph: GraphName,
        subject: NamedOrBlankNode,
        predicate: NamedNode,
        object: Term,
    },
    /// A statement of its own, beside any that hold the same triple, as a property graph creates an edge: a parallel
    /// edge, or an edge with properties, takes a statement ID.
    InsertEdge {
        graph: GraphName,
        subject: NamedOrBlankNode,
        predicate: NamedNode,
        object: Term,
        properties: Vec<(NamedNode, Term)>,
    },
    /// The triple and every statement that holds it.
    Delete {
        graph: GraphName,
        subject: NamedOrBlankNode,
        predicate: NamedNode,
        object: Term,
    },
    /// One statement, by its ID.
    DeleteStatement { graph: GraphName, id: StatementId },
    /// Every object of the subject's predicate in the graph.
    DeletePattern {
        graph: GraphName,
        subject: NamedOrBlankNode,
        predicate: NamedNode,
    },
    /// Adds a property to a statement, which takes a statement ID if it has none.
    SetProperty {
        statement: StatementRef,
        predicate: NamedNode,
        value: Term,
    },
    /// Removes a statement's property, one value of it or, with no value, every value.
    DeleteProperty {
        statement: StatementRef,
        predicate: NamedNode,
        value: Option<Term>,
    },
    /// A catalog declaration.
    Declare(Declaration),
}
