/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod store_workload_keys;

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_traits_store::{
    Entry, PersistMode, RebuildRecord, Scan, Store, StoreSnapshot, Temporary, WriteBatch,
};

use crate::types::event::Event;
use crate::types::fault_config::FaultConfig;
use crate::types::run_outcome::RunOutcome;
use crate::types::seed::Seed;
use crate::types::sim_context::SimContext;
use crate::types::sim_snapshot::SimSnapshot;
use crate::types::simulation::Simulation;
use crate::types::violation::Violation;

use crate::types::store_workload::store_workload_keys::{KeySpace, PREFIX_DATA, PREFIX_SCHEMA};

/// The meta byte of a delta posting list.
const DELTA: u8 = 0x04;
/// The meta byte of a complete posting list.
const COMPLETE: u8 = 0x08;

/// The store's own workload, over Dgraph-shaped keys: one apply loop writing commit batches of several commits,
/// rollup batches below the newest version, deletions, syncs, prefix drops, drops of everything, bans, the discard
/// timestamp with compaction and the sweep, oversized keys, rebuild records written and removed by commit batches,
/// and a temporary keyspace created, written and dropped; and readers holding snapshots open across steps, of the
/// data or of the temporary keyspace, while they read points, many points, prefix scans in both directions and
/// versions. Every write keeps the store's contract: none lands below a deletion of its key.
#[derive(Debug)]
pub struct StoreWorkload {
    readers: usize,
    next_ts: u64,
    keys: KeySpace,
    /// The newest version written to each key, and its newest deletion. A drop forgets nothing: a drop stopped by a
    /// power loss may not have happened.
    written: BTreeMap<Vec<u8>, (u64, u64)>,
    /// Each reader's open snapshot and the reads it has left.
    open: Vec<Option<(SimSnapshot, u64)>>,
    /// The rebuild records written that no batch has removed yet; a power loss may have taken some.
    records: BTreeSet<u64>,
    next_record: u64,
    /// The temporary keyspace, while one exists.
    temporary: Option<Temporary>,
}

impl StoreWorkload {
    /// The workload with `readers` readers beside the apply loop.
    pub fn new(readers: usize) -> Self {
        Self {
            readers,
            next_ts: 1,
            keys: KeySpace::new(),
            written: BTreeMap::new(),
            open: (0..readers).map(|_| None).collect(),
            records: BTreeSet::new(),
            next_record: 1,
            temporary: None,
        }
    }

    /// The apply loop and the readers.
    pub fn actors(&self) -> usize {
        1 + self.readers
    }

    /// Runs the workload with `readers` readers for `steps` steps of `seed`.
    pub fn run(
        seed: Seed,
        faults: FaultConfig,
        steps: u64,
        readers: usize,
    ) -> Result<RunOutcome, Violation> {
        let mut workload = Self::new(readers);
        let actors = workload.actors();
        Simulation::new(seed, faults).run(steps, actors, |ctx, event| workload.step(ctx, event))
    }

    pub fn step(&mut self, ctx: &mut SimContext, event: Event) -> Result<(), Violation> {
        match event {
            Event::Recovered => {
                self.open.iter_mut().for_each(|o| *o = None);
                self.next_ts = self.next_ts.max(ctx.store().applied_ts() + 1);
                self.temporary = None;
            }
            Event::Step(0) => self.write(ctx),
            Event::Step(reader) => self.read(ctx, reader - 1),
        }
        Ok(())
    }

    /// One operation of the apply loop. A refused or failed write ends the step; the simulator recovers the store.
    fn write(&mut self, ctx: &mut SimContext) {
        let store = ctx.store().clone();
        let choice = ctx.rng().below(110);
        let _ = match choice {
            0..55 => {
                let (batch, mode) = self.commit_batch(ctx);
                store.apply(batch, mode)
            }
            55..65 => match self.rollup_batch(ctx) {
                Some(batch) => store.apply(batch, PersistMode::Buffer),
                None => Ok(()),
            },
            65..70 => store.sync(),
            70..74 => {
                let prefix = self.keys.prefix(ctx.rng());
                let entries = self.drop_entries(ctx, &prefix);
                store.drop_prefixes(self.next_ts, &[prefix], entries)
            }
            74..75 => {
                let schema = self.keys.schema_key(0, "name");
                store.drop_all(vec![Entry::put(
                    schema,
                    1,
                    0x01,
                    b"name: string .".to_vec(),
                )])
            }
            75..77 => store.ban_namespace(self.keys.bannable(ctx.rng())),
            77..86 => {
                let discard = ctx.rng().between(store.discard_ts(), store.applied_ts());
                let mut result = store.set_discard_ts(discard);
                if result.is_ok() && ctx.rng().chance(300) {
                    result = store.set_tombstone_horizon(ctx.rng().between(0, discard));
                }
                result
            }
            86..92 => store.compact(),
            92..97 => store.sweep_deletions(),
            97..100 => {
                let mut batch = WriteBatch::new();
                batch.push(Entry::put(
                    vec![PREFIX_DATA; 65_001],
                    self.next_ts,
                    DELTA,
                    Vec::new(),
                ));
                store.apply(batch, PersistMode::Buffer)
            }
            100..103 => {
                let id = self.next_record;
                self.next_record += 1;
                let prefixes = (0..ctx.rng().between(1, 2))
                    .map(|_| self.keys.prefix(ctx.rng()))
                    .collect();
                self.records.insert(id);
                store.write_rebuild_record(&RebuildRecord::new(id, prefixes))
            }
            _ => self.temporary_step(ctx),
        };
    }

    /// One operation on the temporary keyspace: creates one when there is none, else writes a batch of one to four
    /// versions to it, or drops it.
    fn temporary_step(
        &mut self,
        ctx: &mut SimContext,
    ) -> Result<(), ultragraphdb_error_store::StoreError> {
        let store = ctx.store().clone();
        let Some(temporary) = self.temporary.take() else {
            self.temporary = Some(store.create_temporary()?);
            return Ok(());
        };
        if ctx.rng().chance(200) {
            return store.drop_temporary(temporary);
        }
        let mut batch = WriteBatch::new();
        for _ in 0..ctx.rng().between(1, 4) {
            let key = self.keys.key(ctx.rng());
            let version = ctx.rng().between(1, 5);
            batch.push(if ctx.rng().chance(150) {
                Entry::deletion(key, version)
            } else {
                Entry::put(
                    key,
                    version,
                    DELTA,
                    ctx.rng().next_u64().to_be_bytes().to_vec(),
                )
            });
        }
        if ctx.rng().chance(50) {
            batch.push(Entry::put(vec![PREFIX_DATA; 65_001], 1, DELTA, Vec::new()));
        }
        let result = store.apply_temporary(&temporary, batch);
        self.temporary = Some(temporary);
        result
    }

    /// One to three commits at their own timestamps, each of one to four entries, recording the last as applied.
    fn commit_batch(&mut self, ctx: &mut SimContext) -> (WriteBatch, PersistMode) {
        let mut batch = WriteBatch::new();
        for _ in 0..ctx.rng().between(1, 3) {
            let ts = self.next_ts;
            self.next_ts += 1;
            for _ in 0..ctx.rng().between(1, 4) {
                let key = self.keys.key(ctx.rng());
                let value = ctx.rng().next_u64().to_be_bytes().to_vec();
                let entry = match ctx.rng().below(100) {
                    0..70 => Entry::put(key.clone(), ts, DELTA, value),
                    70..85 => Entry::put(key.clone(), ts, COMPLETE, value).with_discard(),
                    _ => Entry::deletion(key.clone(), ts),
                };
                self.record(&entry);
                batch.push(entry);
            }
            batch.set_applied_ts(ts);
        }
        if !self.records.is_empty() && ctx.rng().chance(300) {
            let at = ctx.rng().index(self.records.len());
            if let Some(id) = self.records.iter().nth(at).copied() {
                self.records.remove(&id);
                batch.remove_rebuild_record(id);
            }
        }
        let mode = if ctx.rng().chance(250) {
            PersistMode::Sync
        } else {
            PersistMode::Buffer
        };
        (batch, mode)
    }

    /// A complete version with the discard flag below a key's newest version and above its newest deletion.
    fn rollup_batch(&mut self, ctx: &mut SimContext) -> Option<WriteBatch> {
        let at = ctx.rng().index(self.written.len());
        let (key, (newest, deleted)) = self.written.iter().nth(at).map(|(k, v)| (k.clone(), *v))?;
        let version = ctx.rng().between(deleted + 1, newest.max(deleted + 1));
        let entry = Entry::put(key, version, COMPLETE, b"rolled up".to_vec()).with_discard();
        self.record(&entry);
        let mut batch = WriteBatch::new();
        batch.push(entry);
        Some(batch)
    }

    /// The entries a drop of `prefix` writes once it is empty: none, or one key under it at timestamp 1.
    fn drop_entries(&mut self, ctx: &mut SimContext, prefix: &[u8]) -> Vec<Entry> {
        if prefix.first() != Some(&PREFIX_SCHEMA) || ctx.rng().chance(500) {
            return Vec::new();
        }
        let mut key = prefix.to_vec();
        key.extend_from_slice(b"\x00\x04name");
        vec![Entry::put(key, 1, 0x01, b"name: string .".to_vec())]
    }

    fn record(&mut self, entry: &Entry) {
        let (newest, deleted) = self.written.entry(entry.key().to_vec()).or_default();
        *newest = (*newest).max(entry.version());
        if entry.is_deletion() {
            *deleted = (*deleted).max(entry.version());
        }
    }

    /// One step of a reader: opens a snapshot, near or below the discard timestamp, or reads through its open one,
    /// which it closes after its last read.
    fn read(&mut self, ctx: &mut SimContext, reader: usize) {
        let store = ctx.store().clone();
        let Some(slot) = self.open.get_mut(reader) else {
            return;
        };
        let Some((snapshot, left)) = slot else {
            let ts = ctx
                .rng()
                .between(store.discard_ts().saturating_sub(2), store.applied_ts() + 2);
            let reads = ctx.rng().between(1, 6);
            let opened = match &self.temporary {
                Some(temporary) if ctx.rng().chance(300) => store.temporary_snapshot(temporary),
                _ => store.snapshot(ts),
            };
            *slot = opened.ok().map(|s| (s, reads));
            return;
        };
        match ctx.rng().below(5) {
            0 => {
                let _ = snapshot.get(&self.keys.key(ctx.rng()));
            }
            1 => {
                let keys: Vec<Vec<u8>> = (0..3).map(|_| self.keys.key(ctx.rng())).collect();
                let _ = snapshot.get_many(&keys);
            }
            2 => {
                let _ = snapshot.versions(&self.keys.key(ctx.rng())).count();
            }
            _ => {
                let mut scan = Scan::prefix(self.keys.prefix(ctx.rng()));
                if ctx.rng().chance(400) {
                    scan = scan.reverse();
                }
                if ctx.rng().chance(400) {
                    scan = scan.all_versions();
                }
                if ctx.rng().chance(300) {
                    scan = scan.seek(self.keys.key(ctx.rng()));
                }
                let _ = snapshot.scan(&scan).count();
            }
        }
        *left -= 1;
        if *left == 0 {
            *slot = None;
        }
    }
}
