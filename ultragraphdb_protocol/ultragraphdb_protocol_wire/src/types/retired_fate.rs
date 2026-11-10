/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What became of a retired name, the `fate` byte of a Describe entry of kind 20 (`PROTOCOL.md` §8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RetiredFate {
    /// 1: renamed, the item holding the current name now.
    Renamed,
    /// 2: retired by a split, a union or a drop, holding no item.
    Retired,
}

impl RetiredFate {
    /// The byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            RetiredFate::Renamed => 1,
            RetiredFate::Retired => 2,
        }
    }

    /// The fate the byte `number` names; `None` outside 1 and 2.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(RetiredFate::Renamed),
            2 => Some(RetiredFate::Retired),
            _ => None,
        }
    }
}
