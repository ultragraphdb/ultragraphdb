/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a drop decided in a round removes, by UID: a proposal of the commit step, whose round's apply drops it at the
/// version the step gives it, and the operation its write-set record names (`PROTOCOL.md` §12.2, codes 1 and 4 to 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DropTarget {
    /// The predicate of this UID: its keys in every graph and its catalog entry.
    Predicate(u64),
    /// The graph of this UID: every key of it.
    Graph(u64),
    /// The namespace's data: every quad key, its catalog kept.
    Data,
    /// The namespace: everything, its initial catalog written again.
    Namespace,
    /// Every namespace: every quad key and catalog, the root namespace's initial catalog written again.
    All,
}
