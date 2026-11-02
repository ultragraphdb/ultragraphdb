/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::{Arc, Mutex, PoisonError};

use ultragraphdb_store_sim::{Event, FaultConfig, Invariant, RunOutcome, Seed, Simulation};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{Entry, Executor, PersistMode, Store, WriteBatch};

#[test]
fn test_the_scheduler_picks_every_actor_and_the_executor() -> TestResult {
    let mut sim = Simulation::new(Seed::new(4), FaultConfig::none());
    let picked = Arc::new(Mutex::new(Vec::new()));
    let ran = Arc::new(Mutex::new(0));
    let outcome = sim.run(60, 3, |ctx, event| {
        if let Event::Step(actor) = event {
            picked
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(actor);
            let ran = ran.clone();
            ctx.executor()
                .spawn(move || *ran.lock().unwrap_or_else(PoisonError::into_inner) += 1);
        }
        Ok(())
    })?;
    let picked = picked.lock().unwrap_or_else(PoisonError::into_inner);
    assert!((0..3).all(|a| picked.contains(&a)));
    let steps: Vec<&String> = outcome
        .trace()
        .iter()
        .filter(|l| l.starts_with("step "))
        .collect();
    assert_eq!(steps.len(), 60);
    assert!(steps[0].starts_with("step 0 picks "), "{}", steps[0]);
    // Each step picks one of the three actors, or the executor, numbered 3, which runs one waiting task.
    let picks = steps
        .iter()
        .enumerate()
        .map(|(i, l)| {
            l.strip_prefix(&format!("step {i} picks "))
                .and_then(|p| p.parse::<usize>().ok())
                .ok_or_else(|| l.to_string())
                .context("each step line names its pick")
        })
        .collect::<TestResult<Vec<usize>>>()?;
    assert!(picks.iter().all(|pick| *pick <= 3), "{steps:?}");
    let executor = picks.iter().filter(|pick| **pick == 3).count();
    assert!(executor > 0, "the executor is stepped");
    assert_eq!(
        *ran.lock().unwrap_or_else(PoisonError::into_inner),
        executor
    );
    assert_eq!(picked.len() + executor, 60);
    assert!(
        sim.trace().is_empty(),
        "a run that ends well hands its trace over"
    );
    Ok(())
}

#[test]
fn test_a_power_loss_between_steps_is_followed_by_recovery() -> TestResult {
    let mut sim = Simulation::new(
        Seed::new(4),
        FaultConfig::none().with_power_loss_between_steps(1000),
    );
    let mut events = Vec::new();
    let _outcome: RunOutcome = sim.run(3, 1, |ctx, event| {
        assert!(!ctx.store().needs_recovery());
        events.push((ctx.step(), event));
        Ok(())
    })?;
    assert_eq!(
        events,
        vec![
            (0, Event::Recovered),
            (0, Event::Step(0)),
            (1, Event::Recovered),
            (1, Event::Step(0)),
            (2, Event::Recovered),
            (2, Event::Step(0)),
        ]
    );
    Ok(())
}

#[test]
fn test_a_broken_invariant_before_the_first_step_stops_the_run_at_step_0() -> TestResult {
    let mut sim = Simulation::new(Seed::new(3), FaultConfig::none());
    let mut deletion = WriteBatch::new();
    deletion.push(Entry::deletion(b"k".to_vec(), 9));
    let mut below = WriteBatch::new();
    below.push(Entry::put(b"k".to_vec(), 4, 0, vec![]));
    let store = sim.context().store().clone();
    store.apply(deletion, PersistMode::Buffer)?;
    // The store takes or refuses the write below the deletion; the model, reading its log, finds the breach.
    drop(store.apply(below, PersistMode::Buffer));
    let mut called = false;
    let violation = sim
        .run(10, 1, |_, _| {
            called = true;
            Ok(())
        })
        .err()
        .context("an error")?;
    assert!(!called, "no step runs");
    assert_eq!(
        violation.to_string(),
        "seed 3 step 0 broke the invariant that no write lands below a deletion of the same key: key 6b written at 4 below its deletion at 9; replay it with seed 3"
    );
    Ok(())
}

#[test]
fn test_a_workload_rule_broken_at_recovery_stops_the_run() -> TestResult {
    let mut sim = Simulation::new(
        Seed::new(4),
        FaultConfig::none().with_crash_between_steps(1000),
    );
    let violation = sim
        .run(5, 1, |ctx, event| match event {
            Event::Recovered => Err(ctx.violation("a recovery reloads the schema", "none loaded")),
            Event::Step(_) => Ok(()),
        })
        .err()
        .context("an error")?;
    assert_eq!((violation.step(), violation.detail()), (0, "none loaded"));
    assert_eq!(
        violation.invariant(),
        Invariant::Workload("a recovery reloads the schema")
    );
    assert!(
        sim.trace().iter().any(|l| l == "Crashed"),
        "{:?}",
        sim.trace()
    );
    Ok(())
}

#[test]
fn test_a_crash_between_steps_is_followed_by_recovery() -> TestResult {
    let mut sim = Simulation::new(
        Seed::new(4),
        FaultConfig::none().with_crash_between_steps(1000),
    );
    let recovered = Arc::new(Mutex::new(0));
    let _outcome: RunOutcome = sim.run(5, 1, |ctx, event| {
        assert!(
            !ctx.store().needs_recovery(),
            "a workload never sees a store that needs recovery"
        );
        if event == Event::Recovered {
            *recovered.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        }
        Ok(())
    })?;
    assert_eq!(*recovered.lock().unwrap_or_else(PoisonError::into_inner), 5);
    Ok(())
}

#[test]
fn test_a_workload_rule_stops_the_run_with_its_seed_and_step() -> TestResult {
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
        .err()
        .context("an error")?;
    assert_eq!((violation.seed(), violation.step()), (Seed::new(8), 3));
    assert_eq!(
        (violation.invariant(), violation.detail()),
        (
            Invariant::Workload("a list holds what was written to it"),
            "posting 0x2 missing"
        )
    );
    assert_eq!(
        sim.trace()
            .iter()
            .filter(|l| l.starts_with("step "))
            .count(),
        4
    );
    Ok(())
}

#[test]
fn test_the_context_hands_out_the_runs_parts() -> TestResult {
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
    ctx.store().apply(batch, PersistMode::Buffer)?;
    assert_eq!(ctx.executor().pending(), 0);
    let outcome = sim.run(0, 1, |_, _| Ok(()))?;
    assert_eq!(outcome.contents().versions(b"k").len(), 1);
    Ok(())
}
