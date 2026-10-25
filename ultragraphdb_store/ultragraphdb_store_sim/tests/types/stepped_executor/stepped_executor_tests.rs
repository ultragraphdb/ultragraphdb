/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::{Arc, Mutex};

use ultragraphdb_store_sim::{Seed, SteppedExecutor, Stream};
use ultragraphdb_traits_store::Executor;

fn executor(seed: u64) -> SteppedExecutor {
    SteppedExecutor::new(Seed::new(seed).stream(Stream::Executor))
}

/// The order a stepped executor of `seed` runs `n` tasks in, each step taking the task at `rng.index(len)` and
/// moving the last into its place, as `Vec::swap_remove` does.
fn expected_order(seed: u64, n: usize) -> Vec<usize> {
    let mut rng = Seed::new(seed).stream(Stream::Executor);
    let mut waiting: Vec<usize> = (0..n).collect();
    let mut order = Vec::new();
    while !waiting.is_empty() {
        let at = rng.index(waiting.len());
        order.push(waiting.swap_remove(at));
    }
    order
}

#[test]
fn test_a_spawned_task_waits_until_the_executor_is_stepped() {
    let e = executor(1);
    let ran = Arc::new(Mutex::new(0));
    let counter = ran.clone();
    e.spawn(move || *counter.lock().unwrap() += 1);
    assert_eq!((e.pending(), *ran.lock().unwrap()), (1, 0));
    assert!(e.step());
    assert_eq!((e.pending(), *ran.lock().unwrap()), (0, 1));
    assert!(!e.step());
    assert_eq!(*ran.lock().unwrap(), 1, "a task runs once");
}

#[test]
fn test_the_seed_decides_the_order_tasks_run_in() {
    let order = |seed| {
        let e = executor(seed);
        let done = Arc::new(Mutex::new(Vec::new()));
        (0..6).for_each(|i| {
            let done = done.clone();
            e.spawn(move || done.lock().unwrap().push(i));
        });
        e.run_until_idle();
        assert_eq!(e.pending(), 0);
        Arc::try_unwrap(done).unwrap().into_inner().unwrap()
    };
    for seed in 1..6 {
        assert_eq!(order(seed), expected_order(seed, 6), "seed {seed}");
    }
    assert!((2..20).any(|seed| order(seed) != order(1)));
}

#[test]
fn test_run_until_idle_runs_the_tasks_tasks_spawn() {
    let e = executor(1);
    let done = Arc::new(Mutex::new(0));
    let (inner, counter) = (e.clone(), done.clone());
    e.spawn(move || {
        let counter = counter.clone();
        inner.spawn(move || *counter.lock().unwrap() += 1);
    });
    e.run_until_idle();
    assert_eq!((*done.lock().unwrap(), e.pending()), (1, 0));
    e.run_until_idle();
    assert_eq!(*done.lock().unwrap(), 1, "an idle executor runs nothing");
}

#[test]
fn test_clear_forgets_the_waiting_tasks() {
    let e = executor(1);
    let ran = Arc::new(Mutex::new(false));
    let flag = ran.clone();
    e.spawn(move || *flag.lock().unwrap() = true);
    e.spawn(|| {});
    assert_eq!(format!("{e:?}"), "SteppedExecutor { pending: 2 }");
    e.clear();
    assert_eq!(e.pending(), 0);
    assert!(!e.step());
    assert!(!*ran.lock().unwrap(), "a forgotten task never runs");
    assert_eq!(format!("{e:?}"), "SteppedExecutor { pending: 0 }");
}

#[test]
fn test_run_all_returns_results_in_task_order_whatever_order_they_ran_in() {
    let ran = Mutex::new(Vec::new());
    let tasks: Vec<_> = (0..8)
        .map(|i| {
            let ran = &ran;
            move || {
                ran.lock().unwrap().push(i);
                i * 10
            }
        })
        .collect();
    let e = executor(3);
    assert_eq!(e.run_all(tasks), (0..8).map(|i| i * 10).collect::<Vec<_>>());
    assert_eq!(ran.into_inner().unwrap(), expected_order(3, 8));
    assert_eq!(e.pending(), 0, "run_all spawns nothing");
}

#[test]
fn test_run_all_of_no_tasks_returns_nothing() {
    let none: Vec<fn() -> u8> = Vec::new();
    assert_eq!(executor(1).run_all(none), Vec::<u8>::new());
}

#[test]
fn test_run_all_draws_from_the_stream_steps_draw_from() {
    // The two share one stream: after run_all has drawn for its tasks, stepping continues from where it stopped.
    let e = executor(4);
    let _ = e.run_all(vec![|| 0, || 1, || 2]);
    let mut rng = Seed::new(4).stream(Stream::Executor);
    (0..3).rev().for_each(|len| {
        rng.index(len + 1);
    });
    let done = Arc::new(Mutex::new(Vec::new()));
    (0..5).for_each(|i| {
        let done = done.clone();
        e.spawn(move || done.lock().unwrap().push(i));
    });
    e.run_until_idle();
    let mut waiting: Vec<usize> = (0..5).collect();
    let mut expected = Vec::new();
    while !waiting.is_empty() {
        let at = rng.index(waiting.len());
        expected.push(waiting.swap_remove(at));
    }
    assert_eq!(*done.lock().unwrap(), expected);
}
