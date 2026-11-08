/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How a range ended in an Entries batch (`PROTOCOL.md` §13.14): the byte `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntriesEnd {
    /// 0: more of the range follows.
    More,
    /// 1: the range ended.
    Ended,
    /// 2: the range stopped at the Scan's `limit`.
    Limit,
}

impl EntriesEnd {
    /// The byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            EntriesEnd::More => 0,
            EntriesEnd::Ended => 1,
            EntriesEnd::Limit => 2,
        }
    }

    /// The end the byte `number` names; `None` past 2.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Self> {
        match number {
            0 => Some(EntriesEnd::More),
            1 => Some(EntriesEnd::Ended),
            2 => Some(EntriesEnd::Limit),
            _ => None,
        }
    }
}
