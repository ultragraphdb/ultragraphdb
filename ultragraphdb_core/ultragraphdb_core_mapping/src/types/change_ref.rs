/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphNameRef, NamedNode, NamedNodeRef, NamedOrBlankNodeRef, Term, TermRef};

use crate::types::change::Change;
use crate::types::declaration::Declaration;
use crate::types::statement_ref::StatementRef;

/// One operation of a write with its terms borrowed: from the [`Change`] that owns them, or from the frame that carried
/// them, so a write's path holds no owned copy of a term per change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeRef<'a> {
    /// As [`Change::Insert`].
    Insert {
        /// The graph.
        graph: GraphNameRef<'a>,
        /// The subject.
        subject: NamedOrBlankNodeRef<'a>,
        /// The predicate.
        predicate: NamedNodeRef<'a>,
        /// The object.
        object: TermRef<'a>,
    },
    /// As [`Change::InsertEdge`].
    InsertEdge {
        /// The graph.
        graph: GraphNameRef<'a>,
        /// The subject.
        subject: NamedOrBlankNodeRef<'a>,
        /// The predicate.
        predicate: NamedNodeRef<'a>,
        /// The object.
        object: TermRef<'a>,
        /// The statement's properties.
        properties: Vec<(NamedNodeRef<'a>, TermRef<'a>)>,
    },
    /// As [`Change::Delete`].
    Delete {
        /// The graph.
        graph: GraphNameRef<'a>,
        /// The subject.
        subject: NamedOrBlankNodeRef<'a>,
        /// The predicate.
        predicate: NamedNodeRef<'a>,
        /// The object.
        object: TermRef<'a>,
    },
    /// As [`Change::DeleteStatement`].
    DeleteStatement {
        /// The graph.
        graph: GraphNameRef<'a>,
        /// The reifier naming the statement's ID.
        reifier: NamedNodeRef<'a>,
    },
    /// As [`Change::DeletePattern`].
    DeletePattern {
        /// The graph.
        graph: GraphNameRef<'a>,
        /// The subject.
        subject: NamedOrBlankNodeRef<'a>,
        /// The predicate.
        predicate: NamedNodeRef<'a>,
    },
    /// As [`Change::SetProperty`].
    SetProperty {
        /// The statement.
        statement: StatementView<'a>,
        /// The predicate.
        predicate: NamedNodeRef<'a>,
        /// The property's value.
        value: TermRef<'a>,
    },
    /// As [`Change::DeleteProperty`].
    DeleteProperty {
        /// The statement.
        statement: StatementView<'a>,
        /// The predicate.
        predicate: NamedNodeRef<'a>,
        /// The value removed, or none for every value.
        value: Option<TermRef<'a>>,
    },
    /// As [`Change::Declare`], which a frame never carries to a transaction.
    Declare(&'a Declaration),
}

/// The statement a property change names, its terms borrowed, as [`StatementRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementView<'a> {
    /// The statement by its reifier.
    Reifier {
        /// The graph.
        graph: GraphNameRef<'a>,
        /// The reifier.
        reifier: NamedNodeRef<'a>,
    },
    /// The one statement holding a triple.
    Triple {
        /// The graph.
        graph: GraphNameRef<'a>,
        /// The subject.
        subject: NamedOrBlankNodeRef<'a>,
        /// The predicate.
        predicate: NamedNodeRef<'a>,
        /// The object.
        object: TermRef<'a>,
    },
}

impl StatementRef {
    /// The statement with its terms borrowed.
    #[must_use]
    pub fn as_view(&self) -> StatementView<'_> {
        match self {
            Self::Reifier { graph, reifier } => StatementView::Reifier {
                graph: graph.as_ref(),
                reifier: reifier.as_ref(),
            },
            Self::Triple {
                graph,
                subject,
                predicate,
                object,
            } => StatementView::Triple {
                graph: graph.as_ref(),
                subject: subject.as_ref(),
                predicate: predicate.as_ref(),
                object: object.as_ref(),
            },
        }
    }
}

impl Change {
    /// The change with its terms borrowed.
    #[must_use]
    pub fn as_ref(&self) -> ChangeRef<'_> {
        match self {
            Self::Insert {
                graph,
                subject,
                predicate,
                object,
            } => ChangeRef::Insert {
                graph: graph.as_ref(),
                subject: subject.as_ref(),
                predicate: predicate.as_ref(),
                object: object.as_ref(),
            },
            Self::InsertEdge {
                graph,
                subject,
                predicate,
                object,
                properties,
            } => ChangeRef::InsertEdge {
                graph: graph.as_ref(),
                subject: subject.as_ref(),
                predicate: predicate.as_ref(),
                object: object.as_ref(),
                properties: borrowed(properties),
            },
            Self::Delete {
                graph,
                subject,
                predicate,
                object,
            } => ChangeRef::Delete {
                graph: graph.as_ref(),
                subject: subject.as_ref(),
                predicate: predicate.as_ref(),
                object: object.as_ref(),
            },
            Self::DeleteStatement { graph, reifier } => ChangeRef::DeleteStatement {
                graph: graph.as_ref(),
                reifier: reifier.as_ref(),
            },
            Self::DeletePattern {
                graph,
                subject,
                predicate,
            } => ChangeRef::DeletePattern {
                graph: graph.as_ref(),
                subject: subject.as_ref(),
                predicate: predicate.as_ref(),
            },
            Self::SetProperty {
                statement,
                predicate,
                value,
            } => ChangeRef::SetProperty {
                statement: statement.as_view(),
                predicate: predicate.as_ref(),
                value: value.as_ref(),
            },
            Self::DeleteProperty {
                statement,
                predicate,
                value,
            } => ChangeRef::DeleteProperty {
                statement: statement.as_view(),
                predicate: predicate.as_ref(),
                value: value.as_ref().map(Term::as_ref),
            },
            Self::Declare(declaration) => ChangeRef::Declare(declaration),
        }
    }
}

fn borrowed(properties: &[(NamedNode, Term)]) -> Vec<(NamedNodeRef<'_>, TermRef<'_>)> {
    properties
        .iter()
        .map(|(key, value)| (key.as_ref(), value.as_ref()))
        .collect()
}
