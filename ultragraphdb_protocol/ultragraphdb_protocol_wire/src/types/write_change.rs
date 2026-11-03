/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::declared::Declared;
use crate::types::statement_target::StatementTarget;

/// One change of a Write, in order: its terms term-table indices of the Write, its graph `None` for the default
/// graph. The server session converts it to the mapping crate's `Change`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WriteChange {
    /// Inserts the quad.
    Insert {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The subject's term index.
        subject: u32,
        /// The predicate's term index.
        predicate: u32,
        /// The object's term index.
        object: u32,
    },
    /// Inserts the quad with its edge's properties.
    InsertEdge {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The subject's term index.
        subject: u32,
        /// The predicate's term index.
        predicate: u32,
        /// The object's term index.
        object: u32,
        /// Each property of the edge and its value, term indices.
        properties: Vec<(u32, u32)>,
    },
    /// Deletes the quad.
    Delete {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The subject's term index.
        subject: u32,
        /// The predicate's term index.
        predicate: u32,
        /// The object's term index.
        object: u32,
    },
    /// Deletes the statement holding a statement ID that this reifier names.
    DeleteStatement {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The term index of the statement's reifier's skolem IRI.
        reifier: u32,
    },
    /// Deletes every quad of the subject and predicate.
    DeletePattern {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The subject's term index.
        subject: u32,
        /// The predicate's term index.
        predicate: u32,
    },
    /// Sets a property of a statement.
    SetProperty {
        /// The statement the property belongs to.
        statement: StatementTarget,
        /// The predicate's term index.
        predicate: u32,
        /// The value's term index.
        value: u32,
    },
    /// Deletes a property of a statement, or one value of it.
    DeleteProperty {
        /// The statement the property belongs to.
        statement: StatementTarget,
        /// The predicate's term index.
        predicate: u32,
        /// The value's term index.
        value: Option<u32>,
    },
    /// A declaration.
    Declare(Declared),
}
