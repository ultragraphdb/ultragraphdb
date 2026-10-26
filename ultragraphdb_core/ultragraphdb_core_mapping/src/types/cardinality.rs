/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How many objects a subject may hold of a predicate in a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Cardinality {
    /// One: a write replaces the subject's object.
    Single,
    /// Any number, as RDF holds them.
    #[default]
    Multi,
}
