/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Mutex;

use ultragraphdb_traits_store::Executor;

/// An executor running every task at once, in the caller's thread; `run_all` runs them last to first.
#[derive(Default)]
struct Inline {
    spawned: Mutex<usize>,
}

impl Executor for Inline {
    fn spawn<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        *self.spawned.lock().unwrap() += 1;
        task();
    }

    fn run_all<T, F>(&self, tasks: Vec<F>) -> Vec<T>
    where
        F: FnOnce() -> T + Send,
        T: Send,
    {
        let mut results: Vec<T> = tasks.into_iter().rev().map(|t| t()).collect();
        results.reverse();
        results
    }
}

#[test]
fn test_run_all_returns_results_in_task_order_and_tasks_may_borrow() {
    let base = vec![10, 20, 30];
    let tasks: Vec<_> = (0..3)
        .map(|i| {
            let base = &base;
            move || base[i] + 1
        })
        .collect();
    assert_eq!(Inline::default().run_all(tasks), vec![11, 21, 31]);
}

#[test]
fn test_spawn_takes_an_owned_task() {
    let executor = Inline::default();
    let (tx, rx) = std::sync::mpsc::channel();
    executor.spawn(move || tx.send(7).unwrap());
    assert_eq!(rx.recv().unwrap(), 7);
    assert_eq!(*executor.spawned.lock().unwrap(), 1);
}
