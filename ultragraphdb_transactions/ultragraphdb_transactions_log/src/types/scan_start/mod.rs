/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;

/// What a scan of one segment expects: the cluster and incarnation of the log's rounds, the last checkpoint (its
/// position and checksum), and the round the segment must start with, when the scan knows it.
///
/// The first segment kept is scanned without knowing its first round: it may start below the checkpoint, so its
/// first round is taken as the chain's start when it lies at or below the round after the checkpoint. Every later
/// segment continues the chain from the last round before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScanStart {
    cluster: u64,
    incarnation: u64,
    checkpoint: Position,
    checkpoint_checksum: u64,
    next: Option<(Position, u64)>,
}

impl ScanStart {
    /// The first segment kept of a log of `cluster` and `incarnation` whose last checkpoint is `checkpoint` with
    /// `checkpoint_checksum`.
    pub fn new(
        cluster: u64,
        incarnation: u64,
        checkpoint: Position,
        checkpoint_checksum: u64,
    ) -> Self {
        Self {
            cluster,
            incarnation,
            checkpoint,
            checkpoint_checksum,
            next: None,
        }
    }

    /// The same log, at a segment whose first round must be at `position` with `parent` as its parent.
    pub fn continuing(self, position: Position, parent: u64) -> Self {
        Self {
            next: Some((position, parent)),
            ..self
        }
    }

    /// The cluster every round's header must name.
    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    /// The incarnation of the log's rounds; a round of another is the segment's clean end.
    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }

    /// The last checkpoint's position.
    pub fn checkpoint(&self) -> Position {
        self.checkpoint
    }

    /// The checksum of the round at the checkpoint, the parent of the round after it.
    pub fn checkpoint_checksum(&self) -> u64 {
        self.checkpoint_checksum
    }

    /// The position and parent the segment's first round must have, `None` for the first segment kept.
    pub fn next(&self) -> Option<(Position, u64)> {
        self.next
    }
}
