/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The fixed seed set of the apply workload: with every fault class, power losses during drops and rebuilds among
//! them, and without faults; a seed's replay; and the workload's operations on fjall, closed and reopened.

use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimRng, SteppedExecutor, Stream};

use crate::utils::apply_workload::{
    ApplyWorkload, administer, alter, applier_on, check, expected, initial_schema, write,
};

#[test]
fn test_seeds_without_faults() {
    let (mut checks, mut writes, mut changes, mut drops) = (0, 0, 0, 0);
    for seed in 1..=12 {
        let (_, w) = ApplyWorkload::run(Seed::new(seed), FaultConfig::none(), 300)
            .unwrap_or_else(|v| panic!("{v}"));
        checks += w.checks;
        writes += w.writes;
        changes += w.changes;
        drops += w.drops;
    }
    assert!(checks > 2000, "only {checks} checks");
    assert!(writes > 1000, "only {writes} writes");
    assert!(changes > 500, "only {changes} changes");
    assert!(drops > 50, "only {drops} drops");
}

#[test]
fn test_seeds_with_faults() {
    let (mut checks, mut drops) = (0, 0);
    for seed in 100..=123 {
        let (_, w) = ApplyWorkload::run(Seed::new(seed), ApplyWorkload::faults(), 300)
            .unwrap_or_else(|v| panic!("{v}"));
        checks += w.checks;
        drops += w.drops;
    }
    assert!(checks > 2000, "only {checks} checks");
    assert!(drops > 50, "only {drops} drops");
}

/// Power losses during drops, at every stage, and between the steps of rebuilds.
#[test]
fn test_seeds_with_power_losses_during_drops_and_rebuilds() {
    let faults = FaultConfig::none()
        .with_power_loss_during_drop(300)
        .with_crash_during_drop(100)
        .with_power_loss_between_steps(40);
    let mut drops = 0;
    for seed in 200..=211 {
        let (_, w) =
            ApplyWorkload::run(Seed::new(seed), faults, 300).unwrap_or_else(|v| panic!("{v}"));
        drops += w.drops;
    }
    assert!(drops > 20, "only {drops} drops");
}

#[test]
fn test_a_seed_replays_its_run_and_its_events() {
    let run = || {
        let capture = ultragraphdb_store_sim::LogCapture::new();
        let outcome = capture.capture(|| {
            ApplyWorkload::run(Seed::new(7), ApplyWorkload::faults(), 200).expect("run")
        });
        (outcome, capture.events())
    };
    let (first, first_events) = run();
    let (again, again_events) = run();
    assert_eq!(first.0, again.0);
    assert_eq!(first.1.checks, again.1.checks);
    assert!(first_events.len() > 100, "{} events", first_events.len());
    assert_eq!(first_events, again_events);
}

/// The workload's operations and rule on fjall: every step's indexes agree with the data, across a close and a
/// reopen every hundred steps.
#[test]
fn test_the_workload_rules_hold_on_fjall() {
    let root =
        std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let dir = root.join(format!("apply-workload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let seed = Seed::new(31);
    let mut rng = SimRng::new(31);
    let executor = SteppedExecutor::new(seed.stream(Stream::Executor));
    let mut checks = 0;
    for round in 0..3 {
        let store = FjallStore::open(&dir, Durability::Process).expect("the store opens");
        let a = applier_on(store, executor.clone(), seed).unwrap();
        if round == 0 {
            alter(&a, &initial_schema()).unwrap();
        }
        for step in 0..100 {
            let result = if rng.chance(600) {
                write(&a, &mut rng)
            } else {
                administer(&a, &mut rng).map(|_| ())
            };
            if let Err(e) = result {
                assert!(expected(&e), "step {step}: {e}");
            }
            if rng.chance(300) {
                executor.run_until_idle();
            }
            if let Some(detail) = check(&a).unwrap() {
                panic!("round {round} step {step}: {detail}");
            }
            checks += 1;
        }
        executor.run_until_idle();
        assert!(check(&a).unwrap().is_none());
    }
    assert_eq!(checks, 300);
    let _ = std::fs::remove_dir_all(&dir);
}
