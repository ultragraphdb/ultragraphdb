/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The bytes of Entries frames one turn of a partition builds (`PROTOCOL.md` §13.14). A commit waits for the turn under way before
/// its round is synced, so what a turn reads is added to its latency whole. Building a frame reads, decodes, copies and hashes at
/// around 1 to 2 GB/s, so this is about 130 to 260 µs, below the one sync the commit waits for anyway: 457 µs on the disk the
/// Shard-per-Core note measures, 2,188 syncs a second. Reads so cost a commit under one sync per turn, and a core running only reads
/// still yields to its connections a few thousand times a second. Task 12.5b's measurements check it.
pub const TURN_BYTES: usize = 256 * 1024;

/// What a frame costs a turn at least: its seek and its allocations take about as long as copying this many bytes, so a turn serving
/// many small Scans ends as soon as one serving a few large ones.
const FRAME_COST: usize = 16 * 1024;

/// What a page finding only keys holding nothing costs a turn: 256 keys of about this many bytes between them.
const IDLE_PAGE_COST: usize = 64 * 1024;

/// What one turn may still build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnBudget {
    left: usize,
}

impl Default for TurnBudget {
    fn default() -> Self {
        Self { left: TURN_BYTES }
    }
}

impl TurnBudget {
    /// Whether the turn should build nothing more.
    #[must_use]
    pub fn is_spent(&self) -> bool {
        self.left == 0
    }

    /// The bytes the turn's next frame fills to: what is left, and at least what a frame costs in any case, so a turn's last frame is
    /// not left too small to be worth its seek.
    #[must_use]
    pub fn fill_to(&self) -> usize {
        self.left.max(FRAME_COST)
    }

    /// A frame of `bytes` was built.
    pub fn spend_frame(&mut self, bytes: usize) {
        self.left = self.left.saturating_sub(bytes.max(FRAME_COST));
    }

    /// A page found only keys holding nothing.
    pub fn spend_idle_page(&mut self) {
        self.left = self.left.saturating_sub(IDLE_PAGE_COST);
    }
}
