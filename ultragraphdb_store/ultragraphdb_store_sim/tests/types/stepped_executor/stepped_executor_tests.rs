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
        Arc::try_unwrap(done).unwrap().into_inner().unwrap()
    };
    assert_eq!(order(1), order(1));
    assert!((2..20).any(|seed| order(seed) != order(1)));
    assert_eq!(order(1).len(), 6);
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
    assert_eq!(*done.lock().unwrap(), 1);
}

#[test]
fn test_clear_forgets_the_waiting_tasks() {
    let e = executor(1);
    e.spawn(|| {});
    e.clear();
    assert_eq!(e.pending(), 0);
    assert!(format!("{e:?}").contains("pending: 0"));
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
    assert_eq!(
        executor(3).run_all(tasks),
        (0..8).map(|i| i * 10).collect::<Vec<_>>()
    );
    let ran = ran.into_inner().unwrap();
    assert_eq!(ran.len(), 8);
    assert_ne!(
        ran,
        (0..8).collect::<Vec<_>>(),
        "seed 3 runs them out of order"
    );
}
