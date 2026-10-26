/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a predicate's objects may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum PredicateKind {
    /// IRIs and blank nodes: an edge in the property-graph view.
    Edge,
    /// Literals: a property in the property-graph view.
    Property,
    /// Either, as RDF allows.
    #[default]
    Either,
}
