/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use ultragraphdb_store_kv::ThreadExecutor;
use ultragraphdb_traits_store::Executor;

#[test]
fn test_a_spawned_task_runs() {
    let (tx, rx) = mpsc::channel();
    ThreadExecutor::new().spawn(move || tx.send(7).unwrap());
    assert_eq!(rx.recv().unwrap(), 7);
}

#[test]
fn test_run_all_returns_results_in_task_order_and_tasks_may_borrow() {
    let inputs = [3u64, 1, 2];
    let tasks: Vec<_> = inputs.iter().map(|n| move || n * 10).collect();
    assert_eq!(ThreadExecutor.run_all(tasks), vec![30, 10, 20]);
    assert!(ThreadExecutor.run_all(Vec::<fn() -> u8>::new()).is_empty());
}

#[test]
fn test_a_panicking_task_panics_the_call_with_its_payload_once_every_task_finished() {
    let finished = AtomicUsize::new(0);
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let finished = &finished;
        let tasks: Vec<_> = [true, false]
            .into_iter()
            .map(|panics| {
                move || {
                    if panics {
                        panic!("boom");
                    }
                    std::thread::sleep(Duration::from_millis(20));
                    finished.fetch_add(1, Ordering::SeqCst);
                }
            })
            .collect();
        ThreadExecutor.run_all(tasks)
    }));
    let payload = result.expect_err("the call panics");
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"boom"));
    assert_eq!(
        finished.load(Ordering::SeqCst),
        1,
        "the other task ran to its end first"
    );
}
