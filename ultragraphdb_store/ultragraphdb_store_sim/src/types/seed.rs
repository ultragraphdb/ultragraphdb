/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::sim_rng::SimRng;
use crate::types::stream::Stream;

/// The one number a simulation run is a function of: every choice the run makes is drawn from a stream derived
/// from it, so running the same seed again replays the run step for step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Seed(u64);

impl Seed {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn value(&self) -> u64 {
        self.0
    }

    /// The generator of `stream`, the same for the same seed and stream.
    pub fn stream(&self, stream: Stream) -> SimRng {
        SimRng::new(self.0 ^ (stream as u64 + 1).wrapping_mul(0xA076_1D64_78BD_642F))
    }
}

impl fmt::Display for Seed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
