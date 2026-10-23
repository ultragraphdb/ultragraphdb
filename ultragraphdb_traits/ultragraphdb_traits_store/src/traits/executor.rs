/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Where background work runs: rollups, index rebuilds and the parallel apply of a batch of mutations. Storage
/// code takes its threads from the executor it is given and starts none of its own, so the simulator can step the
/// work in an order its seed decides; `ultragraphdb_store_kv` runs it on threads.
pub trait Executor {
    /// Starts `task` in the background. It runs once, at a time the executor chooses, and reports its outcome
    /// through what it captures.
    fn spawn<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static;

    /// Runs every task of `tasks`, possibly at once and in any order, and returns their results in the order of
    /// `tasks` once all have finished. The tasks may borrow from the caller.
    fn run_all<T, F>(&self, tasks: Vec<F>) -> Vec<T>
    where
        F: FnOnce() -> T + Send,
        T: Send;
}
