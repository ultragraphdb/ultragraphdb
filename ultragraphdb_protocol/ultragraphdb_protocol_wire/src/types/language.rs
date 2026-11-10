/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a statement is written in, the `language` byte of a Prepare and a Query (`PROTOCOL.md` §7.4, §8.1): a Query is
/// a text alone, so it takes 1 and 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    /// 0: a quad pattern.
    Pattern,
    /// 1: the GQL core.
    Gql,
    /// 2: the SPARQL core.
    Sparql,
    /// 3: UQL, its CRUD subset from stage 6 (`QUERY-ULTRA-QL.md` section 4.1).
    Uql,
}

impl Language {
    /// The byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            Language::Pattern => 0,
            Language::Gql => 1,
            Language::Sparql => 2,
            Language::Uql => 3,
        }
    }

    /// The language the byte `number` names; `None` past 3.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Self> {
        match number {
            0 => Some(Language::Pattern),
            1 => Some(Language::Gql),
            2 => Some(Language::Sparql),
            3 => Some(Language::Uql),
            _ => None,
        }
    }
}
