/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod stepped_executor_executor;

use std::sync::{Arc, Mutex};

use crate::types::sim_rng::SimRng;
use crate::utils::lock::lock;

/// A task waiting in a [`SteppedExecutor`]. Tasks of every kind wait in one queue, so the queue holds them as trait
/// objects.
type Task = Box<dyn FnOnce() + Send>;

/// The simulator's [`ultragraphdb_traits_store::Executor`]: a spawned task waits until the simulator steps the
/// executor, which runs one waiting task, chosen by the executor's stream; `run_all` runs its tasks in the caller's
/// thread, one after another, in an order drawn from the same stream. Clones share one queue.
#[derive(Clone)]
pub struct SteppedExecutor {
    shared: Arc<Mutex<Shared>>,
}

struct Shared {
    tasks: Vec<Task>,
    rng: SimRng,
}

impl SteppedExecutor {
    pub fn new(rng: SimRng) -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared {
                tasks: Vec::new(),
                rng,
            })),
        }
    }

    /// How many spawned tasks wait.
    pub fn pending(&self) -> usize {
        lock(&self.shared).tasks.len()
    }

    /// Runs one waiting task; false when none waits.
    pub fn step(&self) -> bool {
        let task = {
            let mut shared = lock(&self.shared);
            if shared.tasks.is_empty() {
                return false;
            }
            let len = shared.tasks.len();
            let at = shared.rng.index(len);
            shared.tasks.swap_remove(at)
        };
        task();
        true
    }

    /// Runs waiting tasks, those they spawn included, until none waits.
    pub fn run_until_idle(&self) {
        while self.step() {}
    }

    /// Forgets every waiting task, as a crash of the process does.
    pub fn clear(&self) {
        lock(&self.shared).tasks.clear();
    }
}

impl std::fmt::Debug for SteppedExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SteppedExecutor")
            .field("pending", &self.pending())
            .finish()
    }
}
