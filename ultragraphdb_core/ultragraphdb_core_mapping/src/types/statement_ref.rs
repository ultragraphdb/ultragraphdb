/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

/// The statement a property change names: by its reifier, or by its triple when one statement alone holds it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StatementRef {
    /// The statement by its reifier, the IRI naming its statement ID.
    Reifier {
        /// The graph.
        graph: GraphName,
        /// The reifier.
        reifier: NamedNode,
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
