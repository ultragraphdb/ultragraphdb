/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a retired name named, the `item` byte of a Describe entry of kind 20 (`PROTOCOL.md` §8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RetiredItem {
    /// 1: a type.
    Type,
    /// 2: an attribute.
    Attribute,
    /// 3: a role.
    Role,
    /// 4: a navigation name.
    Navigation,
    /// 5: a graph.
    Graph,
}

impl RetiredItem {
    /// The byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            RetiredItem::Type => 1,
            RetiredItem::Attribute => 2,
            RetiredItem::Role => 3,
            RetiredItem::Navigation => 4,
            RetiredItem::Graph => 5,
        }
    }

    /// The item the byte `number` names; `None` outside 1 to 5.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(RetiredItem::Type),
            2 => Some(RetiredItem::Attribute),
            3 => Some(RetiredItem::Role),
            4 => Some(RetiredItem::Navigation),
            5 => Some(RetiredItem::Graph),
            _ => None,
        }
    }
}
