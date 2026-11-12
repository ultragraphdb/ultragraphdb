/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a parameter or a result column holds (`PROTOCOL.md` §8.1), one byte on the wire. A quad pattern's are
/// [`ColumnType::Node`], at its graph, subject and predicate, and [`ColumnType::Term`], at its object; a query text's
/// take every byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ColumnType {
    /// 1: an IRI or a blank node; a query's node, edge, reifier or relation node, by its IRI.
    Node,
    /// 2: any term.
    Term,
    /// 3: `xsd:integer` and its subtypes.
    Integer,
    /// 4: `xsd:decimal`.
    Decimal,
    /// 5: `xsd:double` or `xsd:float`.
    Double,
    /// 6: `xsd:string`, a language-tagged string, or a string a query's enum, IRI, unit or catalog-type value takes.
    String,
    /// 7: `xsd:boolean`.
    Boolean,
    /// 8: `xsd:dateTimeStamp` or `xsd:dateTime`.
    DateTime,
    /// 9: `xsd:date`.
    Date,
    /// 10: `xsd:time`.
    Time,
    /// 11: `xsd:duration` and its subtypes.
    Duration,
    /// 12: a vector literal.
    Vector,
    /// 13: an `rdf:JSON` literal: a list or a record.
    Json,
}

impl ColumnType {
    /// Every type, in the order of its number.
    pub const ALL: [ColumnType; 13] = [
        ColumnType::Node,
        ColumnType::Term,
        ColumnType::Integer,
        ColumnType::Decimal,
        ColumnType::Double,
        ColumnType::String,
        ColumnType::Boolean,
        ColumnType::DateTime,
        ColumnType::Date,
        ColumnType::Time,
        ColumnType::Duration,
        ColumnType::Vector,
        ColumnType::Json,
    ];

    /// The type's number on the wire.
    pub fn number(self) -> u8 {
        ColumnType::ALL
            .iter()
            // The 13 types of `ALL` are numbered from 1, so the number is at most 13 and fits a byte.
            .zip(1..)
            .find(|(t, _)| **t == self)
            .map_or(0, |(_, number)| number)
    }

    /// The type numbered `number`; `None` for a number no type has.
    pub fn from_number(number: u8) -> Option<ColumnType> {
        ColumnType::ALL
            .get(usize::from(number).checked_sub(1)?)
            .copied()
    }

    /// Whether a quad pattern's parameter or result column takes the type: 1 or 2.
    #[must_use]
    pub fn in_pattern(self) -> bool {
        matches!(self, ColumnType::Node | ColumnType::Term)
    }
}
