/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// One position of a quad pattern: unbound, the default graph (the graph position only), bound to a term of the
/// body's term table by its index, or a statement's parameter, numbered in the order the pattern's parameters appear.
///
/// On the wire a slot is 8 bytes: `kind` u32 (0 unbound, 1 the default graph, 2 bound, 3 a parameter) and `value` u32
/// (the bound term's index, or the parameter's number; zero otherwise).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PatternSlot {
    Unbound,
    DefaultGraph,
    Bound(u32),
    Parameter(u32),
}
