/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The level of a transaction's reads (`PROTOCOL.md` §11.11), a byte in Begin's and ComputeRead's command header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadLevel {
    /// 0: one snapshot for every read, at the latest closed epoch or the token's, whichever is higher; the default.
    Snapshot,
    /// 1: the latest state of the one partition the transaction's first read binds it to.
    Latest,
    /// 2: each read at one epoch, never below its floor, so the transaction's reads never go back.
    MonotonicFast,
}

impl ReadLevel {
    /// The level's byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            ReadLevel::Snapshot => 0,
            ReadLevel::Latest => 1,
            ReadLevel::MonotonicFast => 2,
        }
    }

    /// The level of byte `number`; `None` above 2.
    #[must_use]
    pub fn from_number(number: u8) -> Option<ReadLevel> {
        match number {
            0 => Some(ReadLevel::Snapshot),
            1 => Some(ReadLevel::Latest),
            2 => Some(ReadLevel::MonotonicFast),
            _ => None,
        }
    }
}
