/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a Begin naming a token ahead of the view came to (`PROTOCOL.md` §4.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldOutcome {
    /// The close before the token's epoch arrived: the Begin is answered.
    Answered,
    /// Its deadline was reached with that close not arrived: code 24.
    TimedOut,
}
