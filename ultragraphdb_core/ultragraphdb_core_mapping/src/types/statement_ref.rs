/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

use crate::types::statement_id::StatementId;

/// The statement a property change names: by its ID, or by its triple when one statement alone holds it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StatementRef {
    /// The statement by its ID.
    Id {
        /// The graph.
        graph: GraphName,
        /// The statement's ID.
        id: StatementId,
    },
    /// The one statement holding a triple.
    Triple {
        /// The graph.
        graph: GraphName,
        /// The subject.
        subject: NamedOrBlankNode,
        /// The predicate.
        predicate: NamedNode,
        /// The object.
        object: Term,
    },
}
