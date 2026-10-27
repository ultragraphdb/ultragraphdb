/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;

use crate::types::truncation::Truncation;

/// What opening the log found: the last checkpoint, the committed position the scan reached, and the torn tail it
/// cut off, if any. The commit step replays the rounds above the store's applied position from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Recovery {
    checkpoint: Position,
    committed: Position,
    truncation: Option<Truncation>,
}

impl Recovery {
    pub fn new(checkpoint: Position, committed: Position, truncation: Option<Truncation>) -> Self {
        Self {
            checkpoint,
            committed,
            truncation,
        }
    }

    pub fn checkpoint(&self) -> Position {
        self.checkpoint
    }

    pub fn committed(&self) -> Position {
        self.committed
    }

    pub fn truncation(&self) -> Option<Truncation> {
        self.truncation
    }
}
