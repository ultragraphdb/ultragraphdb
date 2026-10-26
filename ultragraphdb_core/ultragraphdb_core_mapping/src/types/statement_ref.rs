/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

use crate::types::statement_id::StatementId;

/// The statement a property change names: by its ID, or by its triple when one statement alone holds it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StatementRef {
    Id {
        graph: GraphName,
        id: StatementId,
    },
    Triple {
        graph: GraphName,
        subject: NamedOrBlankNode,
        predicate: NamedNode,
        object: Term,
    },
}
