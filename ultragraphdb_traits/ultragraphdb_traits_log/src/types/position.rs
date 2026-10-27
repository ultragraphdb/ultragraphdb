/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A round's place in the log. Positions are gapless: the first round is at 1 and each later round at the next
/// position; [`Position::ZERO`] is the place before the first round, a fresh log's tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Position(u64);

impl Position {
    /// The place before the first round.
    pub const ZERO: Self = Self(0);

    pub fn new(position: u64) -> Self {
        Self(position)
    }

    /// The position as a number.
    pub fn get(&self) -> u64 {
        self.0
    }

    /// The position after this one, `None` past `u64::MAX`.
    pub fn next(&self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}
