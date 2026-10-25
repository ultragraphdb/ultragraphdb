/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{
    Breach, Event, FaultConfig, Invariant, Seed, Simulation, Stream, Violation,
};
use ultragraphdb_traits_store::Store;

#[test]
fn test_the_context_draws_from_the_seeds_workload_stream() {
    let mut sim = Simulation::new(Seed::new(8), FaultConfig::none());
    let mut expected = Seed::new(8).stream(Stream::Workload);
    let ctx = sim.context();
    assert_eq!(
        (0..3).map(|_| ctx.rng().next_u64()).collect::<Vec<_>>(),
        (0..3).map(|_| expected.next_u64()).collect::<Vec<_>>()
    );
}

#[test]
fn test_the_contexts_clock_is_its_stores() {
    let mut sim = Simulation::new(Seed::new(8), FaultConfig::none());
    let ctx = sim.context();
    ctx.store().clock().advance(11);
    assert_eq!(ctx.clock().now(), 11);
    ctx.clock().advance(4);
    assert_eq!(ctx.store().clock().now(), 15);
    assert_eq!(ctx.store().applied_ts(), 0);
}

#[test]
fn test_the_step_counts_from_zero_through_a_run() {
    let mut sim = Simulation::new(Seed::new(8), FaultConfig::none());
    let mut seen = Vec::new();
    sim.run(3, 1, |ctx, event| {
        if let Event::Step(_) = event {
            seen.push(ctx.step());
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(seen, [0, 1, 2]);
    assert_eq!(sim.context().step(), 3);
}

#[test]
fn test_a_violation_names_the_seed_the_step_and_the_rule() {
    let mut sim = Simulation::new(Seed::new(8), FaultConfig::none());
    let violation = sim
        .run(5, 1, |ctx, _| {
            if ctx.step() == 2 {
                Err(ctx.violation("lists are sorted", "posting 0x3 before 0x2"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert_eq!(
        violation,
        Violation::new(
            Seed::new(8),
            2,
            Breach::new(
                Invariant::Workload("lists are sorted"),
                "posting 0x3 before 0x2"
            )
        )
    );
    assert_eq!(
        violation.to_string(),
        "seed 8 step 2 broke the invariant that lists are sorted: posting 0x3 before 0x2; replay it with seed 8"
    );
}
