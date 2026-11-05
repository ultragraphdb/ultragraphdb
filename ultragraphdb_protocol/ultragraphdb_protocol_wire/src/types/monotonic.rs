/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a Result batch's rows may read as, its `monotonic` byte (`PROTOCOL.md` §9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Monotonic {
    /// 0: a snapshot read, or a Read or Execute of a pattern.
    #[default]
    Snapshot,
    /// 1: a monotonic-fast read of a query over data that may have been retracted since.
    Retractable,
    /// 2: a monotonic-fast read over immutable data.
    Immutable,
}

impl Monotonic {
    /// The byte.
    pub fn number(self) -> u8 {
        match self {
            Monotonic::Snapshot => 0,
            Monotonic::Retractable => 1,
            Monotonic::Immutable => 2,
        }
    }

    /// The value the byte `number` names; `None` past 2.
    pub fn from_number(number: u8) -> Option<Self> {
        match number {
            0 => Some(Monotonic::Snapshot),
            1 => Some(Monotonic::Retractable),
            2 => Some(Monotonic::Immutable),
            _ => None,
        }
    }
}
