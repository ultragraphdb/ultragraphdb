/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::panic;
use std::thread;

use ultragraphdb_traits_store::Executor;

use crate::types::thread_executor::ThreadExecutor;

impl Executor for ThreadExecutor {
    #[allow(
        clippy::disallowed_methods,
        reason = "the production executor runs background work on threads; the simulator's stepped executor \
                  replaces it so that a run replays"
    )]
    fn spawn<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        // A task reports its outcome through what it captures, so its handle is not kept.
        drop(thread::spawn(task));
    }

    /// A task that panics makes this call panic with its payload once every task has finished, as running the
    /// tasks one after the other on the caller's thread would.
    #[allow(
        clippy::disallowed_methods,
        reason = "the production executor runs the tasks of one batch on scoped threads; the simulator's stepped \
                  executor replaces it so that a run replays"
    )]
    fn run_all<T, F>(&self, tasks: Vec<F>) -> Vec<T>
    where
        F: FnOnce() -> T + Send,
        T: Send,
    {
        let results: Vec<thread::Result<T>> = thread::scope(|scope| {
            let handles: Vec<_> = tasks.into_iter().map(|t| scope.spawn(t)).collect();
            handles.into_iter().map(|h| h.join()).collect()
        });
        results
            .into_iter()
            .map(|r| r.unwrap_or_else(|payload| panic::resume_unwind(payload)))
            .collect()
    }
}
