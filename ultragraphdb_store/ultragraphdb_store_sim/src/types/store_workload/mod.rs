/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod store_workload_keys;
mod store_workload_reads;

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_traits_store::{
    Entry, PersistMode, RebuildRecord, Scan, Store, StoreSnapshot, Temporary, WriteBatch,
};

use crate::types::event::Event;
use crate::types::fault_config::FaultConfig;
use crate::types::run_outcome::RunOutcome;
use crate::types::seed::Seed;
use crate::types::sim_context::SimContext;
use crate::types::sim_rng::SimRng;
use crate::types::sim_snapshot::SimSnapshot;
use crate::types::simulation::Simulation;
use crate::types::stream::Stream;
use crate::types::violation::Violation;

use crate::types::store_workload::store_workload_keys::{CATALOG, FORWARD, KeySpace, PREDICATES};
use crate::types::store_workload::store_workload_reads::ReadShape;

/// The meta byte of a delta posting list.
const DELTA: u8 = 0x04;
/// The meta byte of a complete posting list.
const COMPLETE: u8 = 0x08;
/// The chance per thousand that a reader with an open snapshot first makes one of its earlier reads again.
const REREAD: u32 = 500;
/// The chance per thousand that a commit batch writes keys without a version beside its entries.
const VERSIONLESS_WRITE: u32 = 400;
/// The chance per thousand that a reader, when versionless reads are on, reads a key without a version first.
const VERSIONLESS_READ: u32 = 400;
/// The keys without a version the workload writes and reads: dictionary-shaped, a namespace then a tag and an id.
const VERSIONLESS_KEYS: u64 = 8;

/// A reader's open snapshot, the reads it has left, and each read it made through it with whether it failed.
type OpenRead = (SimSnapshot, u64, Vec<(ReadShape, bool)>);

/// The store's own workload, over keys of the quad layout: one apply loop writing commit batches of several commits,
/// rollup batches below the newest version, deletions, syncs, prefix drops, drops of everything, bans, the discard
/// timestamp with compaction and the sweep, oversized keys, rebuild records written and removed by commit batches,
/// a temporary keyspace created, written and dropped, and keys without a version written and removed in commit
/// batches; and readers holding snapshots open across steps, of the
/// data or of the temporary keyspace, while they read points, many points, prefix scans in both directions and
/// versions, and make earlier reads again through the same snapshot after the apply loop has moved on, and, when
/// versionless reads are on, read keys without a version through their snapshots and through the store. Every
/// write keeps the store's contract: none lands below a deletion of its key.
///
/// The keyspace without versions draws from a stream of its own, so the workload's other draws, and the faults its
/// writes meet, are those of a run without it. Its reads draw read failures from the fault stream, which moves the
/// faults that follow, so they are on only for the seed sets that ask for them.
#[derive(Debug)]
pub struct StoreWorkload {
    readers: usize,
    next_ts: u64,
    keys: KeySpace,
    /// The newest version written to each key, and its newest deletion. A drop forgets nothing: a drop stopped by a
    /// power loss may not have happened.
    written: BTreeMap<Vec<u8>, (u64, u64)>,
    /// Each reader's open snapshot.
    open: Vec<Option<OpenRead>>,
    /// Which earlier reads the readers make again, drawn from the seed's own stream for it so that the workload's
    /// stream draws what it drew without them; taken from the run's seed at the first step.
    rereads: Option<SimRng>,
    /// The rebuild records written that no batch has removed yet; a power loss may have taken some.
    records: BTreeSet<u64>,
    next_record: u64,
    /// The temporary keyspace, while one exists.
    temporary: Option<Temporary>,
    /// The writes and reads of the keyspace without versions, drawn from the seed's stream for them; taken from the
    /// run's seed at the first step.
    versionless: Option<SimRng>,
    versionless_reads: bool,
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
            rereads: None,
            versionless: None,
            versionless_reads: false,
        }
    }

    /// The workload with its readers also reading keys without a version.
    pub fn with_versionless_reads(mut self) -> Self {
        self.versionless_reads = true;
        self
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
        Self::new(readers).run_on(seed, faults, steps)
    }

    /// Runs this workload for `steps` steps of `seed`.
    pub fn run_on(
        mut self,
        seed: Seed,
        faults: FaultConfig,
        steps: u64,
    ) -> Result<RunOutcome, Violation> {
        let actors = self.actors();
        Simulation::new(seed, faults).run(steps, actors, |ctx, event| self.step(ctx, event))
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
                let declaration = self.keys.catalog_key(0, PREDICATES[0]);
                store.drop_all(vec![Entry::put(declaration, 1, 0x01, b"name".to_vec())])
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
                    vec![FORWARD; 65_001],
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
            batch.push(Entry::put(vec![FORWARD; 65_001], 1, DELTA, Vec::new()));
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
        let versionless = self
            .versionless
            .get_or_insert_with(|| ctx.seed().stream(Stream::Versionless));
        if versionless.chance(VERSIONLESS_WRITE) {
            for _ in 0..versionless.between(1, 3) {
                let key = versionless_key(versionless);
                if versionless.chance(100) {
                    batch.remove_versionless(key);
                } else {
                    batch.put_versionless(key, versionless.next_u64().to_be_bytes().to_vec());
                }
            }
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
        if prefix.first() != Some(&CATALOG) || ctx.rng().chance(500) {
            return Vec::new();
        }
        // A namespace's catalog prefix gets the declaration of `name`; a predicate's prefix is its key.
        let key = match prefix.get(1..9) {
            Some(ns) if prefix.len() == 9 => {
                let namespace = ns.iter().fold(0u64, |n, &b| n << 8 | u64::from(b));
                self.keys.catalog_key(namespace, PREDICATES[0])
            }
            _ => prefix.to_vec(),
        };
        vec![Entry::put(key, 1, 0x01, b"name".to_vec())]
    }

    fn record(&mut self, entry: &Entry) {
        let (newest, deleted) = self.written.entry(entry.key().to_vec()).or_default();
        *newest = (*newest).max(entry.version());
        if entry.is_deletion() {
            *deleted = (*deleted).max(entry.version());
        }
    }

    /// One step of a reader: opens a snapshot, near or below the discard timestamp, or reads through its open one,
    /// which it closes after its last read. Before a read it may first make one of its earlier reads again, which
    /// the read count leaves out.
    fn read(&mut self, ctx: &mut SimContext, reader: usize) {
        let store = ctx.store().clone();
        let rereads = self
            .rereads
            .get_or_insert_with(|| ctx.seed().stream(Stream::Rereads));
        let Some(slot) = self.open.get_mut(reader) else {
            return;
        };
        let Some((snapshot, left, made)) = slot else {
            let ts = ctx
                .rng()
                .between(store.discard_ts().saturating_sub(2), store.applied_ts() + 2);
            let reads = ctx.rng().between(1, 6);
            let opened = match &self.temporary {
                Some(temporary) if ctx.rng().chance(300) => store.temporary_snapshot(temporary),
                _ => store.snapshot(ts),
            };
            *slot = opened.ok().map(|s| (s, reads, Vec::new()));
            return;
        };
        if self.versionless_reads {
            let versionless = self
                .versionless
                .get_or_insert_with(|| ctx.seed().stream(Stream::Versionless));
            if versionless.chance(VERSIONLESS_READ) {
                let key = versionless_key(versionless);
                let _ = if versionless.chance(300) {
                    store.get_versionless(&key)
                } else {
                    snapshot.get_versionless(&key)
                };
            }
        }
        made.iter_mut()
            .filter(|(_, failed)| *failed)
            .for_each(|(read, failed)| *failed = read.issue(snapshot));
        if !made.is_empty() && rereads.chance(REREAD) {
            let at = rereads.index(made.len());
            let (read, failed) = &mut made[at];
            *failed = read.issue(snapshot);
        }
        let read = match ctx.rng().below(5) {
            0 => ReadShape::Get(self.keys.key(ctx.rng())),
            1 => ReadShape::GetMany((0..3).map(|_| self.keys.key(ctx.rng())).collect()),
            2 => ReadShape::Scan(Scan::key(self.keys.key(ctx.rng())).all_versions()),
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
                ReadShape::Scan(scan)
            }
        };
        let failed = read.issue(snapshot);
        made.push((read, failed));
        *left -= 1;
        if *left == 0 {
            *slot = None;
        }
    }
}

/// One of the keys without a version the workload draws: namespace 0, the dictionary's IRI tag, then a small id.
fn versionless_key(rng: &mut SimRng) -> Vec<u8> {
    let mut key = 0u64.to_be_bytes().to_vec();
    key.push(0x01);
    key.extend_from_slice(&rng.below(VERSIONLESS_KEYS).to_be_bytes());
    key
}
