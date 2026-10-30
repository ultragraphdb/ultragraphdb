/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a Drop removes, its `kind`.
///
/// | Number | Kind | Removes |
/// |---|---|---|
/// | 1 | Predicate | the predicate's keys and its catalog entry |
/// | 2 | Graph | every key of the graph in the namespace |
/// | 3 | NamespaceData | the namespace's data, keeping its catalog |
/// | 4 | Namespace | everything in the namespace, writing its initial catalog |
/// | 5 | All | every quad key, keeping the dictionary and writing the root namespace's initial catalog |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DropKind {
    Predicate,
    Graph,
    NamespaceData,
    Namespace,
    All,
}

impl DropKind {
    /// Every kind, in the order of its number.
    pub const ALL: [DropKind; 5] = [
        DropKind::Predicate,
        DropKind::Graph,
        DropKind::NamespaceData,
        DropKind::Namespace,
        DropKind::All,
    ];

    pub fn number(self) -> u8 {
        DropKind::ALL
            .iter()
            .position(|k| *k == self)
            .map_or(0, |i| i as u8 + 1)
    }

    /// The kind numbered `number`; `None` for a number no kind has.
    pub fn from_number(number: u8) -> Option<DropKind> {
        DropKind::ALL
            .get(usize::from(number).checked_sub(1)?)
            .copied()
    }

    /// Whether the drop names its target by an IRI in the body's term table: a predicate's or a graph's.
    pub fn names_target(self) -> bool {
        matches!(self, DropKind::Predicate | DropKind::Graph)
    }
}
