/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod violation_display;
mod violation_error;

use crate::types::breach::Breach;
use crate::types::invariant::Invariant;
use crate::types::seed::Seed;

/// The end of a failing run: the invariant broken, at which step, under which seed. Running the seed again
/// replays the run up to the same step.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Violation {
    seed: Seed,
    step: u64,
    breach: Breach,
}

impl Violation {
    pub fn new(seed: Seed, step: u64, breach: Breach) -> Self {
        Self { seed, step, breach }
    }

    pub fn seed(&self) -> Seed {
        self.seed
    }

    pub fn step(&self) -> u64 {
        self.step
    }

    pub fn invariant(&self) -> Invariant {
        self.breach.invariant()
    }

    pub fn detail(&self) -> &str {
        self.breach.detail()
    }
}
