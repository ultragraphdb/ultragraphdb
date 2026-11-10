/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a get-only registration found at the home, its `status` byte in an accepted reply (`PROTOCOL.md` §13.15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReferenceStatus {
    /// 1: absent, an IRI the home's dictionary does not hold or an ID it holds nothing of.
    Absent,
    /// 2: live.
    Live,
    /// 3: deleted, its tombstone held.
    Deleted,
    /// 4: retracted, an instance of a `@bitemporal` type whose `asserted` has ended.
    Retracted,
}

impl ReferenceStatus {
    /// The byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            ReferenceStatus::Absent => 1,
            ReferenceStatus::Live => 2,
            ReferenceStatus::Deleted => 3,
            ReferenceStatus::Retracted => 4,
        }
    }

    /// The status the byte `number` names; `None` outside 1 to 4.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(ReferenceStatus::Absent),
            2 => Some(ReferenceStatus::Live),
            3 => Some(ReferenceStatus::Deleted),
            4 => Some(ReferenceStatus::Retracted),
            _ => None,
        }
    }
}
