/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// One position of a quad pattern: unbound, the default graph (the graph position only), bound to an RDF term in
/// N-Triples text, or a statement's parameter, numbered in the order the pattern's parameters appear.
///
/// On the wire a slot is 8 bytes: `kind` u32 (0 unbound, 1 the default graph, 2 bound, 3 a parameter) and `value` u32
/// (the bound term's index in the pattern's terms, or the parameter's number; zero otherwise).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PatternSlot<T> {
    Unbound,
    DefaultGraph,
    Bound(T),
    Parameter(u32),
}
