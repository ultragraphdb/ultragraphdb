/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The statement a property change names, by terms alone, each a term-table index, the graph `None` for the default
/// graph (`PROTOCOL.md` §4.6): a statement holding a statement ID by its reifier's skolem IRI, the one a result batch
/// named it by; one without by its quad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatementTarget {
    /// The statement of this reifier.
    Reifier {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The term index of the reifier's skolem IRI, an IRI.
        reifier: u32,
    },
    /// The statement of this quad, holding no statement ID.
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
