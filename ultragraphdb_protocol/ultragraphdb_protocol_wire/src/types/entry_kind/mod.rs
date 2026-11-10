/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The kind of a Describe entry, its `kind` byte (`PROTOCOL.md` §8.3): kinds 1 to 18 a catalog version's entries as
/// `CATALOG.md` encodes them, 19 an installed source's text, 20 a retired name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EntryKind {
    /// 1: a schema.
    Schema,
    /// 2: a module.
    Module,
    /// 3: a type.
    Type,
    /// 4: a type's prefix ranges.
    PrefixRanges,
    /// 5: an attribute.
    Attribute,
    /// 6: a predicate.
    Predicate,
    /// 7: a role.
    Role,
    /// 8: a navigation name.
    Navigation,
    /// 9: an enum.
    Enum,
    /// 10: a record.
    Record,
    /// 11: a named attribute type.
    NamedAttributeType,
    /// 12: a quantity.
    Quantity,
    /// 13: a unit.
    Unit,
    /// 14: a derivation.
    Derivation,
    /// 15: a function.
    Function,
    /// 16: a graph.
    Graph,
    /// 17: a type's classification.
    Classification,
    /// 18: the version's report.
    Report,
    /// 19: an installed source, its text by its name.
    Source,
    /// 20: a retired name, by that name.
    RetiredName,
}

/// Every kind, in the order of its number, from 1.
const KINDS: [EntryKind; 20] = [
    EntryKind::Schema,
    EntryKind::Module,
    EntryKind::Type,
    EntryKind::PrefixRanges,
    EntryKind::Attribute,
    EntryKind::Predicate,
    EntryKind::Role,
    EntryKind::Navigation,
    EntryKind::Enum,
    EntryKind::Record,
    EntryKind::NamedAttributeType,
    EntryKind::Quantity,
    EntryKind::Unit,
    EntryKind::Derivation,
    EntryKind::Function,
    EntryKind::Graph,
    EntryKind::Classification,
    EntryKind::Report,
    EntryKind::Source,
    EntryKind::RetiredName,
];

impl EntryKind {
    /// The byte.
    #[must_use]
    pub fn number(self) -> u8 {
        // The kinds' numbers: KINDS lists them in their order, from 1, and holds 20, so the place fits a byte.
        KINDS
            .iter()
            .position(|kind| *kind == self)
            .and_then(|at| u8::try_from(at + 1).ok())
            .unwrap_or(0)
    }

    /// The kind the byte `number` names; `None` outside 1 to 20.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Self> {
        KINDS.get(usize::from(number).checked_sub(1)?).copied()
    }
}
