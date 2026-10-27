/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a parameter or a result column holds: a node (an IRI or a blank node), at a pattern's graph, subject or
/// predicate; or any term, at its object. One byte on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ColumnType {
    /// An IRI or a blank node: 1.
    Node,
    /// Any term: 2.
    Term,
}

impl ColumnType {
    pub fn number(self) -> u8 {
        match self {
            ColumnType::Node => 1,
            ColumnType::Term => 2,
        }
    }

    pub fn from_number(number: u8) -> Option<ColumnType> {
        match number {
            1 => Some(ColumnType::Node),
            2 => Some(ColumnType::Term),
            _ => None,
        }
    }
}
