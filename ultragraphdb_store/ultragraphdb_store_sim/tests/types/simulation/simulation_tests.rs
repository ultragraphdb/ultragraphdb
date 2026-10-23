/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::{Arc, Mutex};

use ultragraphdb_store_sim::{Event, FaultConfig, Invariant, Seed, Simulation};
use ultragraphdb_traits_store::{Entry, Executor, PersistMode, Store, WriteBatch};

#[test]
fn test_the_scheduler_picks_every_actor_and_the_executor() {
    let mut sim = Simulation::new(Seed::new(4), FaultConfig::none());
    let picked = Arc::new(Mutex::new(Vec::new()));
    let ran = Arc::new(Mutex::new(0));
    let outcome = sim
        .run(60, 3, |ctx, event| {
            if let Event::Step(actor) = event {
                picked.lock().unwrap().push(actor);
                let ran = ran.clone();
                ctx.executor().spawn(move || *ran.lock().unwrap() += 1);
            }
            Ok(())
        })
        .unwrap();
    let picked = picked.lock().unwrap();
    assert!((0..3).all(|a| picked.contains(&a)));
    assert!(*ran.lock().unwrap() > 0, "the executor is stepped");
    assert_eq!(
        outcome
            .trace()
            .iter()
            .filter(|l| l.starts_with("step "))
            .count(),
        60
    );
}

#[test]
fn test_a_crash_between_steps_is_followed_by_recovery() {
    let mut sim = Simulation::new(
        Seed::new(4),
        FaultConfig::none().with_crash_between_steps(1000),
    );
    let recovered = Arc::new(Mutex::new(0));
    sim.run(5, 1, |ctx, event| {
        assert!(
            !ctx.store().needs_recovery(),
            "a workload never sees a store that needs recovery"
        );
        if event == Event::Recovered {
            *recovered.lock().unwrap() += 1;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(*recovered.lock().unwrap(), 5);
}

#[test]
fn test_a_workload_rule_stops_the_run_with_its_seed_and_step() {
    let mut sim = Simulation::new(Seed::new(8), FaultConfig::none());
    let violation = sim
        .run(10, 1, |ctx, _| {
            if ctx.step() == 3 {
                return Err(
                    ctx.violation("a list holds what was written to it", "posting 0x2 missing")
                );
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!((violation.seed(), violation.step()), (Seed::new(8), 3));
    assert_eq!(
        violation.invariant(),
        Invariant::Workload("a list holds what was written to it")
    );
    assert_eq!(
        sim.trace()
            .iter()
            .filter(|l| l.starts_with("step "))
            .count(),
        4
    );
}

#[test]
fn test_the_context_hands_out_the_runs_parts() {
    let mut sim = Simulation::new(Seed::new(8), FaultConfig::none());
    let ctx = sim.context();
    assert_eq!(
        (ctx.seed(), ctx.step(), ctx.clock().now()),
        (Seed::new(8), 0, 0)
    );
    let first = ctx.rng().next_u64();
    assert_ne!(first, ctx.rng().next_u64());
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"k".to_vec(), 1, 0, vec![]));
    ctx.store().apply(batch, PersistMode::Buffer).unwrap();
    assert_eq!(ctx.executor().pending(), 0);
    let outcome = sim.run(0, 1, |_, _| Ok(())).unwrap();
    assert_eq!(outcome.contents().versions(b"k").len(), 1);
}
