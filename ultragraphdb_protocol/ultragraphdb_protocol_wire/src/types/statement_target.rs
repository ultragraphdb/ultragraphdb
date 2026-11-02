/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The statement a property change names: by its statement ID, or by its triple's terms, each a term-table index;
/// the graph `None` for the default graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatementTarget {
    /// The statement of this ID.
    Id {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The statement's ID.
        id: u64,
    },
    /// The statement of this triple.
    Triple {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The subject's term index.
        subject: u32,
        /// The predicate's term index.
        predicate: u32,
        /// The object's term index.
        object: u32,
    },
}
