/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::position::Position;

/// What an append hands back: the position of the round it accepted, which completes once that round is durable.
/// The ticket is a value, not a future: the log answers it through
/// [`OrderedLog::poll_ticket`](crate::OrderedLog::poll_ticket), so holding one borrows nothing and the commit step
/// decides the next round while this one's sync is pending. A command is applied once its ticket completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "a round counts only once its ticket completes"]
pub struct DurabilityTicket {
    position: Position,
}

impl DurabilityTicket {
    /// The ticket of the round appended at `position`.
    pub fn new(position: Position) -> Self {
        Self { position }
    }

    /// The position of the round the ticket stands for.
    #[must_use]
    pub fn position(&self) -> Position {
        self.position
    }
}
