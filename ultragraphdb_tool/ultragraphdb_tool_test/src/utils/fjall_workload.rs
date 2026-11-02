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

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::{FjallSnapshot, FjallStore};
use ultragraphdb_store_sim::{
    Contents, Event, FaultConfig, OpRecord, Seed, SimContext, Simulation, StoreModel,
    StoreWorkload, Stream,
};
use ultragraphdb_traits_store::{
    Entry, Item, PersistMode, Scan, Store, StoreSnapshot, Temporary, WriteBatch,
};

use crate::utils::test_store::TestDir;

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
    /// How many versionless reads were replayed, and how many of them found a value.
    versionless_reads: (usize, usize),
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
            versionless_reads: (0, 0),
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
        match record {
            OpRecord::Read { id, key, result } => return self.replay_read(*id, key, result),
            OpRecord::ReadVersionless { id, key, result } => {
                return self.replay_versionless_read(*id, key, result);
            }
            OpRecord::ReadMany { id, keys, result } => {
                return self.replay_read_many(*id, keys, result);
            }
            OpRecord::Scanned { id, scan, items } => return self.replay_scan(*id, scan, items),
            OpRecord::Applied { batch, mode } => {
                let result = self.store().apply(batch.clone(), *mode);
                same("a batch", &format!("{result:?}"), "Ok(())")?;
            }
            OpRecord::Refused { batch, error } => self.replay_refused(batch, error)?,
            OpRecord::Synced => done(self.store().sync())?,
            OpRecord::Dropped {
                ts,
                prefixes,
                entries,
            } => self.replay_drop(*ts, prefixes.as_deref(), entries)?,
            OpRecord::Banned { namespace } => done(self.store().ban_namespace(*namespace))?,
            OpRecord::DiscardSet { ts } => done(self.store().set_discard_ts(*ts))?,
            OpRecord::HorizonSet { ts } => done(self.store().set_tombstone_horizon(*ts))?,
            OpRecord::Compacted => done(self.store().compact())?,
            OpRecord::Swept => done(self.store().sweep_deletions())?,
            OpRecord::RebuildRecorded { record: written } => {
                done(self.store().write_rebuild_record(written))?;
            }
            OpRecord::SnapshotOpened { id, read_ts } => self.replay_snapshot(*id, *read_ts)?,
            OpRecord::SnapshotRefused { read_ts, error } => {
                self.replay_refused_snapshot(*read_ts, error)?;
            }
            OpRecord::SnapshotClosed { id } => {
                self.snapshots.remove(id);
            }
            OpRecord::TemporaryCreated { temporary } => self.replay_create_temporary(*temporary)?,
            OpRecord::TemporaryApplied { temporary, batch } => {
                self.replay_temporary_batch(*temporary, batch, None)?;
            }
            OpRecord::TemporaryRefused {
                temporary,
                batch,
                error,
            } => self.replay_temporary_batch(*temporary, batch, Some(error))?,
            OpRecord::TemporarySnapshotOpened { id, temporary } => {
                self.replay_temporary_snapshot(*id, *temporary)?;
            }
            OpRecord::TemporaryDropped { temporary } => self.replay_drop_temporary(*temporary)?,
            OpRecord::Crashed => self.crash(),
            OpRecord::Recovered => self.store = Some(self.dir.open()),
            OpRecord::BatchFailed { .. }
            | OpRecord::CrashedDuringBatch { .. }
            | OpRecord::WriteWhileFailed { .. }
            | OpRecord::DropInterrupted { .. }
            | OpRecord::RebuildInterrupted { .. }
            | OpRecord::TemporaryFailed { .. }
            | OpRecord::Corrupted { .. }
            | OpRecord::Slowed { .. }
            | OpRecord::PowerLost => {
                return Err(format!("the fjall store cannot replay {record:?}"));
            }
        }
        Ok(record.clone())
    }

    /// The crash between steps, replayed as a close: what the process held is gone.
    fn crash(&mut self) {
        self.temporaries.clear();
        self.snapshots.clear();
        self.store = None;
    }

    fn replay_refused_snapshot(&self, read_ts: u64, error: &StoreError) -> Result<(), String> {
        let result = self.store().snapshot(read_ts).map(|_| ());
        same(
            "a refused snapshot",
            &format!("{result:?}"),
            &refusal(error),
        )
    }

    fn replay_refused(&self, batch: &WriteBatch, error: &StoreError) -> Result<(), String> {
        let result = self.store().apply(batch.clone(), PersistMode::Buffer);
        same("a refused batch", &format!("{result:?}"), &refusal(error))
    }

    fn replay_drop(
        &mut self,
        ts: u64,
        prefixes: Option<&[Vec<u8>]>,
        entries: &[Entry],
    ) -> Result<(), String> {
        match prefixes {
            Some(prefixes) => {
                done(self.store().drop_prefixes(ts, prefixes, entries.to_vec()))?;
                self.fenced += entries.len();
            }
            None => done(self.store().drop_all(entries.to_vec()))?,
        }
        Ok(())
    }

    fn replay_snapshot(&mut self, id: u64, read_ts: u64) -> Result<(), String> {
        let snapshot = self
            .store()
            .snapshot(read_ts)
            .map_err(|e| format!("a snapshot at {read_ts}: {e}"))?;
        self.snapshots.insert(id, snapshot);
        Ok(())
    }

    fn replay_read(
        &self,
        id: u64,
        key: &[u8],
        result: &Result<Option<Item>, StoreError>,
    ) -> Result<OpRecord, String> {
        let got = self.snapshot(id).get(key);
        same("a read", &format!("{got:?}"), &format!("{result:?}"))?;
        Ok(OpRecord::Read {
            id,
            key: key.to_vec(),
            result: got,
        })
    }

    fn replay_versionless_read(
        &mut self,
        id: Option<u64>,
        key: &[u8],
        result: &Result<Option<Vec<u8>>, StoreError>,
    ) -> Result<OpRecord, String> {
        let got = match id {
            Some(id) => self.snapshot(id).get_versionless(key),
            None => self.store().get_versionless(key),
        };
        same(
            "a versionless read",
            &format!("{got:?}"),
            &format!("{result:?}"),
        )?;
        self.versionless_reads.0 += 1;
        self.versionless_reads.1 += usize::from(matches!(got, Ok(Some(_))));
        Ok(OpRecord::ReadVersionless {
            id,
            key: key.to_vec(),
            result: got,
        })
    }

    fn replay_read_many(
        &self,
        id: u64,
        keys: &[Vec<u8>],
        result: &Result<Vec<Option<Item>>, StoreError>,
    ) -> Result<OpRecord, String> {
        let got = self.snapshot(id).get_many(keys);
        same(
            "a read of many keys",
            &format!("{got:?}"),
            &format!("{result:?}"),
        )?;
        Ok(OpRecord::ReadMany {
            id,
            keys: keys.to_vec(),
            result: got,
        })
    }

    fn replay_scan(
        &self,
        id: u64,
        scan: &Scan,
        items: &[Result<Item, StoreError>],
    ) -> Result<OpRecord, String> {
        let got: Vec<_> = self.snapshot(id).scan(scan).collect();
        same(
            &format!("a scan of {scan:?}"),
            &format!("{got:?}"),
            &format!("{items:?}"),
        )?;
        Ok(OpRecord::Scanned {
            id,
            scan: scan.clone(),
            items: got,
        })
    }

    fn replay_create_temporary(&mut self, temporary: u64) -> Result<(), String> {
        let created = self.store().create_temporary().map_err(|e| e.to_string())?;
        self.temporaries.insert(temporary, created);
        Ok(())
    }

    /// A batch to temporary keyspace `temporary`, which the simulated store refused with `refused` if it names one.
    fn replay_temporary_batch(
        &self,
        temporary: u64,
        batch: &WriteBatch,
        refused: Option<&StoreError>,
    ) -> Result<(), String> {
        let result = self
            .store()
            .apply_temporary(self.temporary(temporary), batch.clone());
        match refused {
            None => same("a temporary batch", &format!("{result:?}"), "Ok(())"),
            Some(error) => same(
                "a refused temporary batch",
                &format!("{result:?}"),
                &refusal(error),
            ),
        }
    }

    fn replay_temporary_snapshot(&mut self, id: u64, temporary: u64) -> Result<(), String> {
        let snapshot = self
            .store()
            .temporary_snapshot(self.temporary(temporary))
            .map_err(|e| format!("a temporary snapshot: {e}"))?;
        self.snapshots.insert(id, snapshot);
        Ok(())
    }

    fn replay_drop_temporary(&mut self, temporary: u64) -> Result<(), String> {
        let dropped = self
            .temporaries
            .remove(&temporary)
            .expect("the temporary keyspace exists");
        done(self.store().drop_temporary(dropped))
    }

    /// Compares what fjall holds with what the simulated store holds, version for version.
    fn compare(&mut self, sim: &Contents, sim_store: &impl Store) -> Result<(), String> {
        let Some(store) = self.store.as_ref() else {
            return Ok(());
        };
        let wanted = sim_versions(sim);
        let held = fjall_versions(store)?;
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
        compare_metadata(store, sim, sim_store)
    }
}

/// Whether fjall answered `what` as the simulated store did.
fn same(what: &str, fjall: &str, sim: &str) -> Result<(), String> {
    if fjall == sim {
        Ok(())
    } else {
        Err(format!(
            "{what}: fjall answered {fjall} where the simulated store answered {sim}"
        ))
    }
}

/// How a refusal with `error` prints, to compare with fjall's answer.
fn refusal(error: &StoreError) -> String {
    format!("{:?}", Err::<(), _>(error.clone()))
}

/// The outcome of an operation fjall must perform.
fn done(result: Result<(), StoreError>) -> Result<(), String> {
    result.map_err(|e| e.to_string())
}

/// Every version the simulated store holds, in order.
fn sim_versions(sim: &Contents) -> Vec<Version> {
    sim.keys()
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
        .collect()
}

/// Every version fjall holds, in order.
fn fjall_versions(store: &FjallStore) -> Result<Vec<Version>, String> {
    Ok(store
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
        .collect())
}

/// Compares fjall's applied, discard and horizon timestamps and its rebuild records with the simulated store's.
fn compare_metadata(
    store: &FjallStore,
    sim: &Contents,
    sim_store: &impl Store,
) -> Result<(), String> {
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

/// What a run on both stores did.
#[derive(Debug, Default)]
pub struct Run {
    replayed: usize,
    fenced: usize,
    recoveries: usize,
    versionless_reads: (usize, usize),
}

impl Run {
    /// How many records were replayed on fjall.
    pub fn replayed(&self) -> usize {
        self.replayed
    }

    /// How many entries prefix drops wrote, fenced on both stores.
    pub fn fenced(&self) -> usize {
        self.fenced
    }

    /// How many times fjall was reopened after a crash between steps.
    pub fn recoveries(&self) -> usize {
        self.recoveries
    }

    /// How many versionless reads were replayed, and how many of them found a value.
    pub fn versionless_reads(&self) -> (usize, usize) {
        self.versionless_reads
    }
}

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
    run.versionless_reads = mirror.versionless_reads;
    Ok(run)
}
