/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::damage::Damage;
use crate::types::scanned_round::ScannedRound;

/// What a scan of one segment found: the rounds of the chain, in order, and where and why it stopped. It stops at
/// `end`, the first block that is not the next round: the segment's length when the rounds fill it, a clean end when
/// `damage` is `None`, and damage otherwise. A segment whose length is not a multiple of 4,096 may end inside its last
/// round's padding, which is never checked; `end` is then that round's padded end, short of a block past the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SegmentScan {
    rounds: Vec<ScannedRound>,
    end: u64,
    damage: Option<Damage>,
}

impl SegmentScan {
    pub fn new(rounds: Vec<ScannedRound>, end: u64, damage: Option<Damage>) -> Self {
        Self {
            rounds,
            end,
            damage,
        }
    }

    pub fn rounds(&self) -> &[ScannedRound] {
        &self.rounds
    }

    /// The offset where the scan stopped, a multiple of 4,096, less than a block past the bytes scanned.
    pub fn end(&self) -> u64 {
        self.end
    }

    pub fn damage(&self) -> Option<Damage> {
        self.damage
    }
}
