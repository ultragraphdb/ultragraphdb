/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use crate::utils::micros_since::micros_since;

/// What a partition's core reads its time from: the role's runtime, an [`Instant`] the core began at, or a clock a test moves
/// on past a limit instead of waiting it out.
pub trait CoreClock {
    /// The microseconds since the core began.
    fn now_us(&self) -> u64;
}

/// The role's clock: the microseconds since the instant the core began.
impl CoreClock for Instant {
    fn now_us(&self) -> u64 {
        micros_since(*self)
    }
}
