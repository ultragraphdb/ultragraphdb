/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::Executor;

use crate::types::stepped_executor::SteppedExecutor;
use crate::utils::lock::lock;

impl Executor for SteppedExecutor {
    fn spawn<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        lock(&self.shared).tasks.push(Box::new(task));
    }

    fn run_all<T, F>(&self, tasks: Vec<F>) -> Vec<T>
    where
        F: FnOnce() -> T + Send,
        T: Send,
    {
        let mut waiting: Vec<(usize, F)> = tasks.into_iter().enumerate().collect();
        let mut done: Vec<(usize, T)> = Vec::with_capacity(waiting.len());
        while !waiting.is_empty() {
            let at = lock(&self.shared).rng.index(waiting.len());
            let (index, task) = waiting.swap_remove(at);
            done.push((index, task()));
        }
        done.sort_by_key(|(index, _)| *index);
        done.into_iter().map(|(_, result)| result).collect()
    }
}
