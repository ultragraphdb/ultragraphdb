/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::event::Event;
use crate::types::fault_config::FaultConfig;
use crate::types::run_outcome::RunOutcome;
use crate::types::seed::Seed;
use crate::types::sim_clock::SimClock;
use crate::types::sim_context::SimContext;
use crate::types::sim_rng::SimRng;
use crate::types::sim_store::SimStore;
use crate::types::stepped_executor::SteppedExecutor;
use crate::types::store_model::StoreModel;
use crate::types::stream::Stream;
use crate::types::violation::Violation;

/// A seeded, deterministic run over a [`SimStore`]: at each step the scheduler's stream picks one actor of the
/// workload, or the executor when a task waits, and the process stream may crash the process or cut its power
/// between steps. After every step and every recovery the shadow model checks the store, and the run stops at the
/// first broken invariant with its seed and step. A failed batch, a crash or a power loss is followed by recovery
/// and nothing else.
#[derive(Debug)]
pub struct Simulation {
    context: SimContext,
    model: StoreModel,
    scheduler: SimRng,
    process: SimRng,
    faults: FaultConfig,
    trace: Vec<String>,
}

impl Simulation {
    pub fn new(seed: Seed, faults: FaultConfig) -> Self {
        let store = SimStore::new(seed, faults, SimClock::new());
        let executor = SteppedExecutor::new(seed.stream(Stream::Executor));
        Self {
            context: SimContext::new(seed, store, executor, seed.stream(Stream::Workload)),
            model: StoreModel::new(),
            scheduler: seed.stream(Stream::Scheduler),
            process: seed.stream(Stream::Process),
            faults,
            trace: Vec::new(),
        }
    }

    pub fn context(&mut self) -> &mut SimContext {
        &mut self.context
    }

    /// The trace of a run that stopped at a violation, up to the violation: what to read when replaying its seed.
    pub fn trace(&self) -> &[String] {
        &self.trace
    }

    /// Runs `steps` steps of a workload of `actors` actors, calling `workload` for each step it is picked for and
    /// after each recovery.
    pub fn run<F>(
        &mut self,
        steps: u64,
        actors: usize,
        mut workload: F,
    ) -> Result<RunOutcome, Violation>
    where
        F: FnMut(&mut SimContext, Event) -> Result<(), Violation>,
    {
        self.check()?;
        for _ in 0..steps {
            if self.process.chance(self.faults.crash_between_steps()) {
                self.context.store().crash();
            } else if self.process.chance(self.faults.power_loss_between_steps()) {
                self.context.store().power_loss();
            }
            self.recover_if_needed(&mut workload)?;
            let waiting = usize::from(self.context.executor().pending() > 0);
            let pick = self.scheduler.index(actors + waiting);
            self.trace
                .push(format!("step {} picks {pick}", self.context.step()));
            if pick < actors {
                workload(&mut self.context, Event::Step(pick))?;
            } else {
                self.context.executor().step();
            }
            self.check()?;
            self.recover_if_needed(&mut workload)?;
            self.context.next_step();
        }
        Ok(RunOutcome::new(
            std::mem::take(&mut self.trace),
            self.context.store().contents(),
        ))
    }

    fn recover_if_needed<F>(&mut self, workload: &mut F) -> Result<(), Violation>
    where
        F: FnMut(&mut SimContext, Event) -> Result<(), Violation>,
    {
        if !self.context.store().needs_recovery() {
            return Ok(());
        }
        self.check()?;
        self.context.executor().clear();
        self.context.store().recover();
        self.check()?;
        workload(&mut self.context, Event::Recovered)?;
        self.check()
    }

    /// Feeds the store's log to the model and compares the contents.
    fn check(&mut self) -> Result<(), Violation> {
        let log = self.context.store().take_log();
        self.trace.extend(log.iter().map(|op| format!("{op:?}")));
        let at = |breach| Violation::new(self.context.seed(), self.context.step(), breach);
        self.model.observe(&log).map_err(at)?;
        self.model
            .check(&self.context.store().contents())
            .map_err(at)
    }
}
