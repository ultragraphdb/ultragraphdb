/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;

use crate::types::damage::Damage;

/// A torn tail the recovery cut off: the round expected at `position` was damaged at byte `offset` of segment
/// `segment`, no complete round followed it, and the log now ends before it, so every round from `position` on is cut
/// off. A round whose write the drive tore while reporting success was acknowledged and is lost with the truncation;
/// this report is how it is named: it, and any torn round after it, lie at `position` or later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Truncation {
    position: Position,
    segment: u64,
    offset: u64,
    damage: Damage,
}

impl Truncation {
    pub fn new(position: Position, segment: u64, offset: u64, damage: Damage) -> Self {
        Self {
            position,
            segment,
            offset,
            damage,
        }
    }

    /// The position of the round cut off: its header's when it verified, the one after the log's new tail otherwise.
    pub fn position(&self) -> Position {
        self.position
    }

    pub fn segment(&self) -> u64 {
        self.segment
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn damage(&self) -> Damage {
        self.damage
    }
}
