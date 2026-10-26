/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The schema's workload on the storage simulator: an apply loop writing schema and type entries in batches,
//! deleting predicates and types and syncing, and a reader loading the schema state, over a fixed seed set with
//! every fault class but corrupted values. Its shadow rules: a load between failures holds every definition
//! written and none deleted, and the state loaded after a recovery is the state after some batch at or after the
//! last sync, which a later load then keeps to.
//!
//! Corrupted values are left out: a flipped byte inside a name decodes to another name, which no decoder of an
//! unchecksummed protobuf can tell from the original.

use std::collections::BTreeMap;

use ultragraphdb_core_catalog::{SchemaMode, SchemaState, schema_entry, type_entry};
use ultragraphdb_proto_pb::pb;
use ultragraphdb_store_sim::{
    Event, FaultConfig, RunOutcome, Seed, SimContext, Simulation, Violation,
};
use ultragraphdb_traits_store::{PersistMode, Store, WriteBatch};

/// A schema state's predicates and types by name.
type Dump = (
    BTreeMap<String, pb::SchemaUpdate>,
    BTreeMap<String, pb::TypeUpdate>,
);

const PREDICATES: [&str; 5] = ["0-name", "0-age", "0-friend", "5-name", "5-age"];
const TYPES: [&str; 3] = ["0-Person", "0-Pet", "5-Animal"];
/// Loads after a recovery give up after this many failed reads in a row, which the fault rates make unreachable.
const LOAD_ATTEMPTS: usize = 64;

fn dump(state: &SchemaState) -> Dump {
    (
        state
            .predicates()
            .into_iter()
            .filter_map(|p| state.get(SchemaMode::Query, &p).map(|s| (p, s)))
            .collect(),
        state
            .types()
            .into_iter()
            .filter_map(|t| state.get_type(&t).map(|s| (t, s)))
            .collect(),
    )
}

#[derive(Debug, Default)]
struct SchemaWorkload {
    next_ts: u64,
    /// The process's schema state, which deletes go through; loaded afresh after every recovery.
    memory: SchemaState,
    /// The states the store may hold, oldest first: one per batch written or failed since the last recovery, after
    /// the state the recovery loaded.
    history: Vec<Dump>,
    /// The first state of `history` a recovery may return to: the newest one a sync covered.
    synced: usize,
    /// How many loads compared a state, and how many recoveries resolved one.
    compared: u64,
    recoveries: u64,
}

impl SchemaWorkload {
    fn new() -> Self {
        Self {
            next_ts: 1,
            history: vec![Dump::default()],
            ..Self::default()
        }
    }

    fn run(seed: Seed, faults: FaultConfig, steps: u64) -> (Result<RunOutcome, Violation>, Self) {
        let mut workload = Self::new();
        let outcome =
            Simulation::new(seed, faults).run(steps, 2, |ctx, event| workload.step(ctx, event));
        (outcome, workload)
    }

    fn step(&mut self, ctx: &mut SimContext, event: Event) -> Result<(), Violation> {
        match event {
            Event::Step(0) => {
                self.write(ctx);
                Ok(())
            }
            Event::Step(_) => self.read(ctx),
            Event::Recovered => self.recover(ctx),
        }
    }

    fn newest(&self) -> Dump {
        self.history.last().cloned().unwrap_or_default()
    }

    /// One operation of the apply loop. The state it makes joins `history` whether or not the write succeeded: a
    /// failed batch may be present after the recovery that follows.
    fn write(&mut self, ctx: &mut SimContext) {
        let store = ctx.store().clone();
        let ts = self.next_ts;
        let mut next = self.newest();
        let written = match ctx.rng().below(100) {
            0..55 => {
                let (batch, mode) = self.batch(ctx, ts, &mut next);
                let synced = mode == PersistMode::Sync;
                store.apply(batch, mode).map(|()| synced).map_err(drop)
            }
            55..70 => {
                let name = PREDICATES[ctx.rng().index(PREDICATES.len())];
                next.0.remove(name);
                self.memory
                    .delete(&store, name, ts)
                    .map(|()| false)
                    .map_err(drop)
            }
            70..80 => {
                let name = TYPES[ctx.rng().index(TYPES.len())];
                next.1.remove(name);
                self.memory
                    .delete_type(&store, name, ts)
                    .map(|()| false)
                    .map_err(drop)
            }
            _ => {
                if store.sync().is_ok() {
                    self.synced = self.history.len() - 1;
                }
                return;
            }
        };
        self.next_ts += 1;
        self.history.push(next);
        if written == Ok(true) {
            self.synced = self.history.len() - 1;
        }
    }

    /// One to three predicates and up to one type, all at `ts`, written into `next` in batch order.
    fn batch(&self, ctx: &mut SimContext, ts: u64, next: &mut Dump) -> (WriteBatch, PersistMode) {
        let mut batch = WriteBatch::new();
        for _ in 0..ctx.rng().between(1, 3) {
            let name = PREDICATES[ctx.rng().index(PREDICATES.len())];
            let update = pb::SchemaUpdate {
                predicate: name.to_string(),
                value_type: i32::try_from(ctx.rng().below(10)).unwrap_or_default(),
                tokenizer: if ctx.rng().chance(300) {
                    vec!["exact".into()]
                } else {
                    Vec::new()
                },
                list: ctx.rng().chance(400),
                count: ctx.rng().chance(200),
                upsert: ctx.rng().chance(200),
                ..Default::default()
            };
            batch.push(schema_entry(&update, ts).expect("a namespaced predicate"));
            next.0.insert(name.to_string(), update);
        }
        if ctx.rng().chance(400) {
            let name = TYPES[ctx.rng().index(TYPES.len())];
            let fields = (0..ctx.rng().between(0, 2))
                .map(|_| pb::SchemaUpdate {
                    predicate: PREDICATES[ctx.rng().index(PREDICATES.len())].to_string(),
                    ..Default::default()
                })
                .collect();
            let update = pb::TypeUpdate {
                type_name: name.to_string(),
                fields,
            };
            batch.push(type_entry(&update, ts).expect("a namespaced type"));
            next.1.insert(name.to_string(), update);
        }
        batch.set_applied_ts(ts);
        let mode = if ctx.rng().chance(250) {
            PersistMode::Sync
        } else {
            PersistMode::Buffer
        };
        (batch, mode)
    }

    /// A reader's load: between failures it holds the newest state. A failed read is skipped.
    fn read(&mut self, ctx: &mut SimContext) -> Result<(), Violation> {
        let state = SchemaState::new();
        if state.load_from_db(ctx.store()).is_err() {
            return Ok(());
        }
        self.compared += 1;
        let (got, want) = (dump(&state), self.newest());
        if got != want {
            return Err(ctx.violation(
                "a load holds every definition written and none deleted",
                format!("loaded {got:?} where {want:?} was written"),
            ));
        }
        Ok(())
    }

    /// After a recovery, the state loaded is one of `history` at or after the last sync; the rest is forgotten.
    fn recover(&mut self, ctx: &mut SimContext) -> Result<(), Violation> {
        let state = SchemaState::new();
        let loaded = (0..LOAD_ATTEMPTS).any(|_| state.load_from_db(ctx.store()).is_ok());
        if !loaded {
            return Err(ctx.violation(
                "the schema loads after a recovery",
                format!("{LOAD_ATTEMPTS} loads failed in a row"),
            ));
        }
        let got = dump(&state);
        let Some(at) = self.history.iter().rposition(|d| *d == got) else {
            return Err(ctx.violation(
                "a recovered schema is the state after a batch",
                format!("loaded {got:?}, which no batch made"),
            ));
        };
        if at < self.synced {
            return Err(ctx.violation(
                "a synced schema survives a recovery",
                format!(
                    "loaded the state after batch {at}, below the synced {}",
                    self.synced
                ),
            ));
        }
        self.history.truncate(at + 1);
        self.memory = state;
        self.recoveries += 1;
        self.next_ts = self.next_ts.max(ctx.store().applied_ts() + 1);
        Ok(())
    }
}

fn faults() -> FaultConfig {
    FaultConfig::mixed().with_corruption(0)
}

#[test]
fn the_fixed_seeds_keep_the_schema() {
    let (mut compared, mut recoveries) = (0, 0);
    for seed in 0..48 {
        let (outcome, workload) = SchemaWorkload::run(Seed::new(seed), faults(), 300);
        if let Err(violation) = outcome {
            panic!("{violation}");
        }
        compared += workload.compared;
        recoveries += workload.recoveries;
    }
    // Evidence that the seeds exercised both rules.
    assert!(compared > 2_000, "{compared} loads compared");
    assert!(recoveries > 50, "{recoveries} recoveries");
}

#[test]
fn a_seed_replays() {
    let (first, a) = SchemaWorkload::run(Seed::new(17), faults(), 300);
    let (again, b) = SchemaWorkload::run(Seed::new(17), faults(), 300);
    assert_eq!(first.expect("no violation"), again.expect("no violation"));
    assert_eq!((a.compared, a.recoveries), (b.compared, b.recoveries));
}

#[test]
fn a_store_that_loses_a_synced_batch_is_caught() {
    // The rule is live: a workload that forgets a sync it made, by claiming a later one, sees the recovery break it.
    let mut workload = SchemaWorkload::new();
    let outcome = Simulation::new(Seed::new(3), FaultConfig::none()).run(40, 2, |ctx, event| {
        if event == Event::Recovered {
            return workload.recover(ctx);
        }
        workload.write(ctx);
        if workload.history.len() > 3 {
            // Claim the newest state synced, then cut the power before any sync covers it.
            workload.synced = workload.history.len() - 1;
            ctx.store().power_loss();
        }
        Ok(())
    });
    let violation = outcome.expect_err("the claimed sync is not durable");
    assert!(
        violation
            .to_string()
            .contains("a synced schema survives a recovery")
            || violation
                .to_string()
                .contains("a recovered schema is the state after a batch"),
        "{violation}"
    );
}
