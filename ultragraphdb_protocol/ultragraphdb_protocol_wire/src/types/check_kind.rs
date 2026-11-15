/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a Check asks for a session's request (`PROTOCOL.md` §13.24).
///
/// | Number | Kind |
/// |---|---|
/// | 1 | Asks: a request that records nothing in a partition's table, a Recall, or the question after every partition answered a Recall with 18; it marks nothing |
/// | 2 | Marks: a request that may record there, a Commit, a Drop, a one-frame write or an Ingest; passing, it marks the session in flight |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CheckKind {
    /// A check that marks nothing.
    Asks,
    /// A check that, passing, marks the session in flight.
    Marks,
}

impl CheckKind {
    /// The kind's number, 1 or 2.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            CheckKind::Asks => 1,
            CheckKind::Marks => 2,
        }
    }

    /// The kind numbered `number`; `None` for a number no kind has.
    #[must_use]
    pub fn from_number(number: u8) -> Option<CheckKind> {
        match number {
            1 => Some(CheckKind::Asks),
            2 => Some(CheckKind::Marks),
            _ => None,
        }
    }
}
