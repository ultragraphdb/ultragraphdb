/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a Drop removes, its `kind`.
///
/// | Number | Kind | Removes |
/// |---|---|---|
/// | 1 | Predicate | the predicate's keys and its catalog entry |
/// | 2 | Graph | every key of the graph in the tenant |
/// | 3 | TenantData | the tenant's data, keeping its catalog |
/// | 4 | Tenant | everything in the tenant, writing its initial catalog |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DropKind {
    /// The predicate's keys and its catalog entry.
    Predicate,
    /// Every key of the graph in the tenant.
    Graph,
    /// The tenant's data, keeping its catalog.
    TenantData,
    /// Everything in the tenant, writing its initial catalog.
    Tenant,
}

impl DropKind {
    /// Every kind, in the order of its number.
    pub const ALL: [DropKind; 4] = [
        DropKind::Predicate,
        DropKind::Graph,
        DropKind::TenantData,
        DropKind::Tenant,
    ];

    /// The kind's number, 1 to 4.
    pub fn number(self) -> u8 {
        DropKind::ALL
            .iter()
            .zip(1..)
            .find(|(k, _)| **k == self)
            .map_or(0, |(_, number)| number)
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
