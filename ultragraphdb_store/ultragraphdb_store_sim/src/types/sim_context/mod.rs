/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::breach::Breach;
use crate::types::invariant::Invariant;
use crate::types::seed::Seed;
use crate::types::sim_clock::SimClock;
use crate::types::sim_rng::SimRng;
use crate::types::sim_store::SimStore;
use crate::types::stepped_executor::SteppedExecutor;
use crate::types::violation::Violation;

/// What a workload works with at each step: the store, the executor and the clock of the run, and the workload's
/// own stream.
#[derive(Debug)]
pub struct SimContext {
    seed: Seed,
    step: u64,
    store: SimStore,
    executor: SteppedExecutor,
    rng: SimRng,
}

impl SimContext {
    pub(crate) fn new(seed: Seed, store: SimStore, executor: SteppedExecutor, rng: SimRng) -> Self {
        Self {
            seed,
            step: 0,
            store,
            executor,
            rng,
        }
    }

    pub fn seed(&self) -> Seed {
        self.seed
    }

    /// The number of the step running.
    pub fn step(&self) -> u64 {
        self.step
    }

    pub(crate) fn next_step(&mut self) {
        self.step += 1;
    }

    pub fn store(&self) -> &SimStore {
        &self.store
    }

    pub fn executor(&self) -> &SteppedExecutor {
        &self.executor
    }

    pub fn clock(&self) -> &SimClock {
        self.store.clock()
    }

    /// The workload's stream.
    pub fn rng(&mut self) -> &mut SimRng {
        &mut self.rng
    }

    /// The violation of a workload's own rule `rule` at this step.
    pub fn violation(&self, rule: &'static str, detail: impl Into<String>) -> Violation {
        Violation::new(
            self.seed,
            self.step,
            Breach::new(Invariant::Workload(rule), detail),
        )
    }
}
