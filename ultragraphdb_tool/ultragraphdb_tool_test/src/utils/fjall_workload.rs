/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The simulator's store workload run on fjall: every operation the workload performs on the simulated store is
//! performed on the fjall store as well, in the same order. Both logs go to a shadow model of their own, which checks
//! every read and refusal against the store's specification; every answer fjall gives must equal the simulated
//! store's; and after every step both stores must hold the same versions and the same metadata.
//!
//! Faults the fjall store cannot be made to suffer (failed batches, power loss, failed and corrupted reads, slow
//! operations) stay with the simulator's own runs; the crash between steps is replayed as a close and a reopen.

use ultragraphdb_store_sim::{
    Event, FaultConfig, OpRecord, Seed, SimContext, Simulation, StoreModel, StoreWorkload, Stream,
};

use crate::types::mirror::Mirror;
use crate::types::run::Run;

/// Runs the store workload for `steps` steps of `seed` on the simulated store and fjall at once, as
/// `Simulation::run` schedules it.
pub fn run_both(seed: u64, faults: FaultConfig, steps: u64, readers: usize) -> Result<Run, String> {
    let seed = Seed::new(seed);
    let mut sim = Simulation::new(seed, faults);
    let mut workload = StoreWorkload::new(readers).with_versionless_reads();
    let actors = workload.actors();
    let mut scheduler = seed.stream(Stream::Scheduler);
    let mut process = seed.stream(Stream::Process);
    let mut sim_model = StoreModel::new();
    let mut fjall_model = StoreModel::new();
    let mut mirror = Mirror::new(&format!("workload-{}", seed.value()))?;
    let mut recoveries = 0usize;
    let at = |step: u64, what: String| format!("seed {seed} step {step}: {what}");

    let check = |ctx: &mut SimContext,
                 fjall_mirror: &mut Mirror,
                 sim_shadow: &mut StoreModel,
                 fjall_shadow: &mut StoreModel,
                 step: u64|
     -> Result<(), String> {
        let log = ctx.store().take_log();
        let recovered = log.contains(&OpRecord::Recovered);
        let mut fjall_log = Vec::with_capacity(log.len());
        for record in &log {
            fjall_log.push(fjall_mirror.replay(record).map_err(|e| at(step, e))?);
        }
        sim_shadow
            .observe(&log)
            .map_err(|b| at(step, format!("the simulated store broke {b:?}")))?;
        let contents = ctx.store().contents();
        sim_shadow
            .check(&contents)
            .map_err(|b| at(step, format!("the simulated store broke {b:?}")))?;
        if recovered {
            // The model waits for the recovered contents to decide which outcome the crash took; the simulated
            // store's decided it, and fjall must hold the same.
            *fjall_shadow = sim_shadow.clone();
        } else {
            fjall_shadow
                .observe(&fjall_log)
                .map_err(|b| at(step, format!("fjall broke {b:?}")))?;
        }
        fjall_mirror
            .compare(&contents, ctx.store())
            .map_err(|e| at(step, e))
    };

    for step in 0..steps {
        let ctx = sim.context();
        if process.chance(faults.crash_between_steps()) {
            ctx.store().crash();
        }
        if ctx.store().needs_recovery() {
            ctx.store().recover();
            recoveries += 1;
            check(ctx, &mut mirror, &mut sim_model, &mut fjall_model, step)?;
            workload
                .step(ctx, Event::Recovered)
                .map_err(|v| v.to_string())?;
        }
        let pick = scheduler.index(actors);
        workload
            .step(ctx, Event::Step(pick))
            .map_err(|v| v.to_string())?;
        check(ctx, &mut mirror, &mut sim_model, &mut fjall_model, step)?;
    }
    Ok(Run::new(
        mirror.replayed,
        mirror.fenced,
        recoveries,
        mirror.versionless_reads,
    ))
}
