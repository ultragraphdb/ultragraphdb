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

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_store_kv::{FjallSnapshot, FjallStore};
use ultragraphdb_store_sim::{
    Contents, Event, FaultConfig, OpRecord, Seed, SimContext, Simulation, StoreModel,
    StoreWorkload, Stream,
};
use ultragraphdb_traits_store::{Store, StoreSnapshot, Temporary};

use crate::test_store::TestDir;

/// One stored version as both stores are compared: key, version, meta byte, discard flag, deletion, value.
type Version = (Vec<u8>, u64, u8, bool, bool, Vec<u8>);

/// The fjall store beside a simulation, with the snapshots its readers hold.
struct Mirror {
    dir: TestDir,
    store: Option<FjallStore>,
    snapshots: BTreeMap<u64, FjallSnapshot>,
    /// The fjall store's temporary keyspaces by the simulated store's numbers for them.
    temporaries: BTreeMap<u64, Temporary>,
    /// The versions prefix drops wrote, which fjall's compaction never collects and the simulated store's may.
    /// How many entries prefix drops wrote, fenced on both stores.
    fenced: usize,
    /// How many records were replayed.
    replayed: usize,
}

impl Mirror {
    fn new(name: &str) -> Self {
        let dir = TestDir::new(name);
        let store = Some(dir.open());
        Self {
            dir,
            store,
            snapshots: BTreeMap::new(),
            temporaries: BTreeMap::new(),
            fenced: 0,
            replayed: 0,
        }
    }

    fn store(&self) -> &FjallStore {
        self.store.as_ref().expect("the fjall store is open")
    }

    fn snapshot(&self, id: u64) -> &FjallSnapshot {
        self.snapshots
            .get(&id)
            .expect("the reader's fjall snapshot is open")
    }

    fn temporary(&self, id: u64) -> &Temporary {
        self.temporaries
            .get(&id)
            .expect("the fjall store's temporary keyspace exists")
    }

    /// Performs `record` on fjall, checks that fjall answers as the simulated store did, and returns the record of
    /// what fjall answered.
    fn replay(&mut self, record: &OpRecord) -> Result<OpRecord, String> {
        self.replayed += 1;
        let same = |what: &str, fjall: String, sim: String| {
            if fjall == sim {
                Ok(())
            } else {
                Err(format!(
                    "{what}: fjall answered {fjall} where the simulated store answered {sim}"
                ))
            }
        };
        let fjall = match record {
            OpRecord::Applied { batch, mode } => {
                let result = self.store().apply(batch.clone(), *mode);
                same("a batch", format!("{result:?}"), "Ok(())".into())?;
                record.clone()
            }
            OpRecord::Refused { batch, error } => {
                let result = self.store().apply(
                    batch.clone(),
                    ultragraphdb_traits_store::PersistMode::Buffer,
                );
                same(
                    "a refused batch",
                    format!("{result:?}"),
                    format!("{:?}", Err::<(), _>(error.clone())),
                )?;
                record.clone()
            }
            OpRecord::Synced => {
                self.store().sync().map_err(|e| e.to_string())?;
                record.clone()
            }
            OpRecord::Dropped {
                ts,
                prefixes,
                entries,
            } => {
                match prefixes {
                    Some(prefixes) => {
                        self.store()
                            .drop_prefixes(*ts, prefixes, entries.clone())
                            .map_err(|e| e.to_string())?;
                        self.fenced += entries.len();
                    }
                    None => {
                        self.store()
                            .drop_all(entries.clone())
                            .map_err(|e| e.to_string())?;
                    }
                }
                record.clone()
            }
            OpRecord::Banned { namespace } => {
                self.store()
                    .ban_namespace(*namespace)
                    .map_err(|e| e.to_string())?;
                record.clone()
            }
            OpRecord::DiscardSet { ts } => {
                self.store()
                    .set_discard_ts(*ts)
                    .map_err(|e| e.to_string())?;
                record.clone()
            }
            OpRecord::HorizonSet { ts } => {
                self.store()
                    .set_tombstone_horizon(*ts)
                    .map_err(|e| e.to_string())?;
                record.clone()
            }
            OpRecord::Compacted => {
                self.store().compact().map_err(|e| e.to_string())?;
                record.clone()
            }
            OpRecord::Swept => {
                self.store().sweep_deletions().map_err(|e| e.to_string())?;
                record.clone()
            }
            OpRecord::SnapshotOpened { id, read_ts } => {
                let snapshot = self
                    .store()
                    .snapshot(*read_ts)
                    .map_err(|e| format!("a snapshot at {read_ts}: {e}"))?;
                self.snapshots.insert(*id, snapshot);
                record.clone()
            }
            OpRecord::SnapshotRefused { read_ts, error } => {
                let result = self.store().snapshot(*read_ts).map(|_| ());
                same(
                    "a refused snapshot",
                    format!("{result:?}"),
                    format!("{:?}", Err::<(), _>(error.clone())),
                )?;
                record.clone()
            }
            OpRecord::SnapshotClosed { id } => {
                self.snapshots.remove(id);
                record.clone()
            }
            OpRecord::Read { id, key, result } => {
                let got = self.snapshot(*id).get(key);
                same("a read", format!("{got:?}"), format!("{result:?}"))?;
                OpRecord::Read {
                    id: *id,
                    key: key.clone(),
                    result: got,
                }
            }
            OpRecord::ReadVersionless { id, key, result } => {
                let got = match id {
                    Some(id) => self.snapshot(*id).get_versionless(key),
                    None => self.store().get_versionless(key),
                };
                same(
                    "a versionless read",
                    format!("{got:?}"),
                    format!("{result:?}"),
                )?;
                OpRecord::ReadVersionless {
                    id: *id,
                    key: key.clone(),
                    result: got,
                }
            }
            OpRecord::ReadMany { id, keys, result } => {
                let got = self.snapshot(*id).get_many(keys);
                same(
                    "a read of many keys",
                    format!("{got:?}"),
                    format!("{result:?}"),
                )?;
                OpRecord::ReadMany {
                    id: *id,
                    keys: keys.clone(),
                    result: got,
                }
            }
            OpRecord::Scanned { id, scan, items } => {
                let got: Vec<_> = self.snapshot(*id).scan(scan).collect();
                same(
                    &format!("a scan of {scan:?}"),
                    format!("{got:?}"),
                    format!("{items:?}"),
                )?;
                OpRecord::Scanned {
                    id: *id,
                    scan: scan.clone(),
                    items: got,
                }
            }
            OpRecord::RebuildRecorded { record: written } => {
                self.store()
                    .write_rebuild_record(written)
                    .map_err(|e| e.to_string())?;
                record.clone()
            }
            OpRecord::TemporaryCreated { temporary } => {
                let created = self.store().create_temporary().map_err(|e| e.to_string())?;
                self.temporaries.insert(*temporary, created);
                record.clone()
            }
            OpRecord::TemporaryApplied { temporary, batch } => {
                let result = self
                    .store()
                    .apply_temporary(self.temporary(*temporary), batch.clone());
                same("a temporary batch", format!("{result:?}"), "Ok(())".into())?;
                record.clone()
            }
            OpRecord::TemporaryRefused {
                temporary,
                batch,
                error,
            } => {
                let result = self
                    .store()
                    .apply_temporary(self.temporary(*temporary), batch.clone());
                same(
                    "a refused temporary batch",
                    format!("{result:?}"),
                    format!("{:?}", Err::<(), _>(error.clone())),
                )?;
                record.clone()
            }
            OpRecord::TemporarySnapshotOpened { id, temporary } => {
                let snapshot = self
                    .store()
                    .temporary_snapshot(self.temporary(*temporary))
                    .map_err(|e| format!("a temporary snapshot: {e}"))?;
                self.snapshots.insert(*id, snapshot);
                record.clone()
            }
            OpRecord::TemporaryDropped { temporary } => {
                let dropped = self
                    .temporaries
                    .remove(temporary)
                    .expect("the temporary keyspace exists");
                self.store()
                    .drop_temporary(dropped)
                    .map_err(|e| e.to_string())?;
                record.clone()
            }
            OpRecord::Crashed => {
                self.temporaries.clear();
                self.snapshots.clear();
                self.store = None;
                record.clone()
            }
            OpRecord::Recovered => {
                self.store = Some(self.dir.open());
                record.clone()
            }
            other => return Err(format!("the fjall store cannot replay {other:?}")),
        };
        Ok(fjall)
    }

    /// Compares what fjall holds with what the simulated store holds, version for version.
    fn compare(&mut self, sim: &Contents, sim_store: &impl Store) -> Result<(), String> {
        let Some(store) = self.store.as_ref() else {
            return Ok(());
        };
        let wanted: Vec<Version> = sim
            .keys()
            .iter()
            .flat_map(|(key, versions)| {
                versions.iter().map(|v| {
                    (
                        key.clone(),
                        v.version(),
                        v.meta(),
                        v.discard(),
                        v.is_deletion(),
                        v.value().to_vec(),
                    )
                })
            })
            .collect();
        let held: Vec<Version> = store
            .dump()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|e| {
                let value = e.value().unwrap_or_default().to_vec();
                (
                    e.key().to_vec(),
                    e.version(),
                    e.meta(),
                    e.discard(),
                    e.is_deletion(),
                    value,
                )
            })
            .collect();
        if held != wanted {
            let wanted_set: BTreeSet<&Version> = wanted.iter().collect();
            let held_set: BTreeSet<&Version> = held.iter().collect();
            let extra: Vec<&&Version> = held_set.difference(&wanted_set).collect();
            let missing: Vec<&&Version> = wanted_set.difference(&held_set).collect();
            return Err(format!(
                "fjall lacks {missing:?} and holds besides {extra:?}"
            ));
        }
        let versionless = sim.versionless();
        let held_versionless = store.dump_versionless().map_err(|e| e.to_string())?;
        if &held_versionless != versionless {
            return Err(format!(
                "fjall's keyspace without versions holds {held_versionless:?} where the simulated store's holds \
                 {versionless:?}"
            ));
        }
        let meta = |s: &dyn Fn() -> (u64, u64, u64)| s();
        let fjall = meta(&|| {
            (
                store.applied_ts(),
                store.discard_ts(),
                store.tombstone_horizon(),
            )
        });
        let simulated = meta(&|| {
            (
                sim_store.applied_ts(),
                sim_store.discard_ts(),
                sim_store.tombstone_horizon(),
            )
        });
        if fjall != simulated {
            return Err(format!(
                "fjall's applied, discard and horizon {fjall:?} where the simulated store's are {simulated:?}"
            ));
        }
        let records: BTreeMap<u64, Vec<Vec<u8>>> = store
            .rebuild_records()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| (r.id(), r.into_prefixes()))
            .collect();
        if &records != sim.rebuild_records() {
            return Err(format!(
                "fjall holds the rebuild records {:?} where the simulated store holds {:?}",
                records.keys().collect::<Vec<_>>(),
                sim.rebuild_records().keys().collect::<Vec<_>>()
            ));
        }
        Ok(())
    }
}

/// What a run on both stores did.
#[derive(Debug, Default)]
struct Run {
    replayed: usize,
    fenced: usize,
    recoveries: usize,
}

/// Runs the store workload for `steps` steps of `seed` on the simulated store and fjall at once, as
/// `Simulation::run` schedules it.
fn run_both(seed: u64, faults: FaultConfig, steps: u64, readers: usize) -> Result<Run, String> {
    let seed = Seed::new(seed);
    let mut sim = Simulation::new(seed, faults);
    let mut workload = StoreWorkload::new(readers).with_versionless_reads();
    let actors = workload.actors();
    let mut scheduler = seed.stream(Stream::Scheduler);
    let mut process = seed.stream(Stream::Process);
    let mut sim_model = StoreModel::new();
    let mut fjall_model = StoreModel::new();
    let mut mirror = Mirror::new(&format!("workload-{}", seed.value()));
    let mut run = Run::default();
    let at = |step: u64, what: String| format!("seed {seed} step {step}: {what}");

    let check = |ctx: &mut SimContext,
                 mirror: &mut Mirror,
                 sim_model: &mut StoreModel,
                 fjall_model: &mut StoreModel,
                 step: u64|
     -> Result<(), String> {
        let log = ctx.store().take_log();
        let recovered = log.contains(&OpRecord::Recovered);
        let mut fjall_log = Vec::with_capacity(log.len());
        for record in &log {
            fjall_log.push(mirror.replay(record).map_err(|e| at(step, e))?);
        }
        sim_model
            .observe(&log)
            .map_err(|b| at(step, format!("the simulated store broke {b:?}")))?;
        let contents = ctx.store().contents();
        sim_model
            .check(&contents)
            .map_err(|b| at(step, format!("the simulated store broke {b:?}")))?;
        if recovered {
            // The model waits for the recovered contents to decide which outcome the crash took; the simulated
            // store's decided it, and fjall must hold the same.
            *fjall_model = sim_model.clone();
        } else {
            fjall_model
                .observe(&fjall_log)
                .map_err(|b| at(step, format!("fjall broke {b:?}")))?;
        }
        mirror
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
            run.recoveries += 1;
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
    run.replayed = mirror.replayed;
    run.fenced = mirror.fenced;
    Ok(run)
}

#[test]
fn test_the_store_workload_passes_the_same_checks_on_fjall() {
    let runs: Vec<Run> = (1..=12)
        .map(|seed| run_both(seed, FaultConfig::none(), 300, 2).unwrap_or_else(|e| panic!("{e}")))
        .collect();
    let replayed: usize = runs.iter().map(|r| r.replayed).sum();
    let fenced: usize = runs.iter().map(|r| r.fenced).sum();
    // Evidence that the run did something: a skip and a pass look alike.
    println!(
        "replayed {replayed} operations on fjall over 12 seeds; {fenced} fenced entries written by prefix drops"
    );
    assert!(replayed > 12 * 300);
}

#[test]
fn test_the_store_workload_survives_crashes_on_fjall() {
    let runs: Vec<Run> = (101..=106)
        .map(|seed| {
            run_both(
                seed,
                FaultConfig::none().with_crash_between_steps(30),
                300,
                2,
            )
            .unwrap_or_else(|e| panic!("{e}"))
        })
        .collect();
    let recoveries: usize = runs.iter().map(|r| r.recoveries).sum();
    println!("reopened fjall {recoveries} times over 6 seeds");
    assert!(recoveries >= 6);
}
