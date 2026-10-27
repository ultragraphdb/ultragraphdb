/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;

/// Why a block where the next round should be is not that round, when it is not empty either: what a scan reports
/// at the place it stopped. A block of zeros, or a valid round of another incarnation, is the clean end of the
/// segment's rounds and no damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Damage {
    /// The block does not hold a round's header: its `size` is out of bounds, or its checksum, command, reserved
    /// bytes or cluster fail. Nothing in it names a position.
    Header,
    /// A round of this log whose header verifies at `position`, which is not the next position or whose parent is
    /// not the previous round's checksum.
    Chain { position: Position },
    /// The chunk table of the round at `position` fails `checksum_body`: no chunk is trusted.
    ChunkTable { position: Position },
    /// Chunk `index`, body bytes `start` to `end`, of the round at `position` does not match its leaf.
    Chunk {
        position: Position,
        index: usize,
        start: usize,
        end: usize,
    },
}
