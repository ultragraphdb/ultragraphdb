/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How a home takes a Reference's registrations, its `mode` byte (`PROTOCOL.md` §13.15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum ReferenceMode {
    /// 0: get or create each entity's ID.
    #[default]
    GetOrCreate,
    /// 1: get each entity and create none, answering its status, for a participant check.
    GetOnly,
}

impl ReferenceMode {
    /// The byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            ReferenceMode::GetOrCreate => 0,
            ReferenceMode::GetOnly => 1,
        }
    }

    /// The mode the byte `number` names; `None` past 1.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Self> {
        match number {
            0 => Some(ReferenceMode::GetOrCreate),
            1 => Some(ReferenceMode::GetOnly),
            _ => None,
        }
    }
}
