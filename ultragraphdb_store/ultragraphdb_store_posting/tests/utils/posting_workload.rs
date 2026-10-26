/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The posting lists' workload on the storage simulator, with its shadow model. The apply loop commits batches of
//! requests, each request reading the writes of the requests before it in its batch, rolls lists up directly and
//! through the rollup worker on the stepped executor, drops keys, advances the cache's epoch, evicts cache entries,
//! syncs, and moves the discard timestamp and compacts; readers read keys at timestamps from the discard timestamp
//! to beyond the newest commit, through the cache and around it. The model holds every key's operations by commit
//! timestamp, and every read must return what the model says it holds at its timestamp, through the cache or not.
//!
//! The simulator's own model checks the store underneath: whole batches, survivors a prefix, the applied
//! timestamp the highest commit timestamp held.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::sync::Arc;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_proto_pb::pb::{Posting, posting};
use ultragraphdb_store_posting::{List, ListsConfig, PostingError, PostingErrorEnum};
use ultragraphdb_store_sim::{
    Event, FaultConfig, RunOutcome, Seed, SimContext, SimListCache, Simulation, Stream, Violation,
};
use ultragraphdb_traits_store::{PersistMode, Store};

use crate::utils::helpers::{TestLists, del_uid, del_value, delete_all, key, set_uid, set_value};

/// The rule a read breaks when it differs from the model.
pub const READ_MATCHES_MODEL: &str =
    "a posting-list read returns the list the model holds at its timestamp";
/// The rule a read through the cache breaks when it differs from the read around it.
pub const CACHE_MATCHES_STORE: &str = "a read through the cache returns what the store holds";
/// The rule a request breaks when it does not see the writes of the requests before it in its batch.
pub const STACKED_WRITES_SEEN: &str =
    "a request reads the writes of the requests before it in its batch";
/// The rule broken by an error no fault explains.
pub const NO_UNEXPECTED_ERROR: &str =
    "a posting-list operation fails only for a fault the simulator injected";

/// The split size of the workload's rollups: small, so that the lists split.
const MAX_LIST_SIZE: usize = 256;
/// The uids a list may hold: enough for more than one block, so that lists can split.
const UIDS: u64 = 600;

/// One operation of a request on a key.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    Set(u64),
    Del(u64),
    SetValue(Vec<u8>),
    DelValue(Vec<u8>),
    DeleteAll,
}

impl Op {
    fn posting(&self) -> Posting {
        match self {
            Op::Set(uid) => set_uid(*uid),
            Op::Del(uid) => del_uid(*uid),
            Op::SetValue(v) => set_value(std::str::from_utf8(v).unwrap_or_default()),
            Op::DelValue(v) => del_value(std::str::from_utf8(v).unwrap_or_default()),
            Op::DeleteAll => delete_all(),
        }
    }
}

/// What a list holds: its reference uids and its values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Held {
    uids: BTreeSet<u64>,
    values: BTreeSet<Vec<u8>>,
}

impl Held {
    fn apply(&mut self, op: &Op) {
        match op {
            Op::Set(uid) => {
                self.uids.insert(*uid);
            }
            Op::Del(uid) => {
                self.uids.remove(uid);
            }
            Op::SetValue(v) => {
                self.values.insert(v.clone());
            }
            Op::DelValue(v) => {
                self.values.remove(v);
            }
            Op::DeleteAll => *self = Held::default(),
        }
    }

    /// What `list` holds at `read_ts`.
    fn of(list: &List, read_ts: u64) -> Result<Held, PostingError> {
        let mut held = Held::default();
        list.iterate(read_ts, 0, |p| {
            if p.posting_type == posting::PostingType::Ref as i32 {
                held.uids.insert(p.uid);
            } else {
                held.values.insert(p.value.clone());
            }
            Ok::<_, PostingError>(ControlFlow::Continue(()))
        })?;
        Ok(held)
    }
}

/// The shadow model: every key's operations by commit timestamp.
#[derive(Debug, Clone, Default)]
struct Model {
    keys: BTreeMap<Vec<u8>, BTreeMap<u64, Vec<Op>>>,
}

impl Model {
    fn held(&self, key: &[u8], read_ts: u64) -> Held {
        let mut held = Held::default();
        if let Some(commits) = self.keys.get(key) {
            commits
                .range(..=read_ts)
                .flat_map(|(_, ops)| ops)
                .for_each(|op| held.apply(op));
        }
        held
    }

    fn commit(&mut self, key: &[u8], ts: u64, ops: Vec<Op>) {
        self.keys.entry(key.to_vec()).or_default().insert(ts, ops);
    }

    /// Forgets the commits a recovery did not keep.
    fn truncate(&mut self, applied_ts: u64) {
        for commits in self.keys.values_mut() {
            commits.retain(|ts, _| *ts <= applied_ts);
        }
    }

    fn drop_key(&mut self, key: &[u8]) {
        self.keys.remove(key);
    }
}

/// A batch whose write failed: its requests' operations by key and its first commit timestamp. Whether it
/// survives is known after the recovery.
type Pending = (u64, Vec<BTreeMap<Vec<u8>, Vec<Op>>>);

/// The posting-list workload of one run.
pub struct PostingWorkload {
    keys: Vec<Vec<u8>>,
    lists: Option<Arc<TestLists>>,
    model: Model,
    pending: Option<Pending>,
    dropping: Option<Vec<u8>>,
    reads: usize,
    rollups: usize,
}

impl Default for PostingWorkload {
    fn default() -> Self {
        Self::new()
    }
}

impl PostingWorkload {
    /// The workload over its five keys: three uid lists and two scalar values.
    pub fn new() -> Self {
        let uid_keys = (1..=3).map(|uid| key("friend", uid));
        let value_keys = (1..=2).map(|uid| key("name", uid));
        Self {
            keys: uid_keys.chain(value_keys).collect(),
            lists: None,
            model: Model::default(),
            pending: None,
            dropping: None,
            reads: 0,
            rollups: 0,
        }
    }

    /// The faults of a posting-list run: every class but corruption, which a posting list cannot always detect.
    pub fn faults() -> FaultConfig {
        FaultConfig::mixed().with_corruption(0)
    }

    /// Runs the workload with `readers` readers for `steps` steps of `seed`: the outcome, and how many reads it
    /// compared and rollups it wrote.
    pub fn run(
        seed: Seed,
        faults: FaultConfig,
        steps: u64,
        readers: usize,
    ) -> Result<(RunOutcome, usize, usize), Violation> {
        let mut workload = Self::new();
        let outcome = Simulation::new(seed, faults)
            .run(steps, readers + 1, |ctx, event| workload.step(ctx, event))?;
        Ok((outcome, workload.reads, workload.rollups))
    }

    fn lists(&mut self, ctx: &SimContext) -> Arc<TestLists> {
        self.lists
            .get_or_insert_with(|| {
                let cache = SimListCache::new(4, ctx.seed().stream(Stream::Cache));
                Arc::new(TestLists::new(
                    ctx.store().clone(),
                    cache,
                    ListsConfig::new(PersistMode::Buffer, MAX_LIST_SIZE),
                ))
            })
            .clone()
    }

    pub fn step(&mut self, ctx: &mut SimContext, event: Event) -> Result<(), Violation> {
        match event {
            Event::Recovered => {
                self.recovered(ctx);
                Ok(())
            }
            Event::Step(0) => self.write(ctx),
            Event::Step(_) => self.read(ctx),
        }
    }

    /// A new process: new lists over the reopened store, and the model told what survived.
    fn recovered(&mut self, ctx: &SimContext) {
        self.lists = None;
        let applied = ctx.store().applied_ts();
        if let Some((first, requests)) = self.pending.take() {
            for (i, ops) in requests.into_iter().enumerate() {
                for (key, ops) in ops {
                    self.model.commit(&key, first + i as u64, ops);
                }
            }
        }
        self.model.truncate(applied);
        if let Some(key) = self.dropping.take() {
            let contents = ctx.store().contents();
            if contents.versions(&key).is_empty() {
                self.model.drop_key(&key);
            }
        }
    }

    fn key(&self, ctx: &mut SimContext) -> Vec<u8> {
        let at = ctx.rng().index(self.keys.len());
        self.keys[at].clone()
    }

    fn is_uid_key(&self, key: &[u8]) -> bool {
        self.keys[..3].iter().any(|k| k == key)
    }

    /// One operation of the apply loop.
    fn write(&mut self, ctx: &mut SimContext) -> Result<(), Violation> {
        let lists = self.lists(ctx);
        let result = match ctx.rng().below(100) {
            0..50 => match self.commit(ctx, &lists) {
                Ok(Some(detail)) => return Err(ctx.violation(STACKED_WRITES_SEEN, detail)),
                Ok(None) => Ok(()),
                Err(error) => Err(error),
            },
            50..57 => {
                lists.spawn_rollups(ctx.executor(), ctx.step());
                Ok(())
            }
            57..64 => {
                let key = self.key(ctx);
                lists.rollup_key(&key).map(|written| {
                    self.rollups += usize::from(written);
                })
            }
            64..68 => lists.sync(),
            68..71 => {
                let key = self.key(ctx);
                self.dropping = Some(key.clone());
                let split_prefix = [&[0x04][..], &key[1..]].concat();
                let ts = lists.fresh_ts();
                let result = lists.drop_prefixes(ts, &[key.clone(), split_prefix], Vec::new());
                if result.is_ok() {
                    self.dropping = None;
                    self.model.drop_key(&key);
                }
                result
            }
            71..78 => {
                let applied = lists.applied_ts();
                let discard = ctx.rng().between(lists.store().discard_ts(), applied);
                lists
                    .set_discard_ts(discard)
                    .and_then(|()| Ok(lists.store().compact()?))
            }
            78..82 => {
                lists.advance_epoch();
                Ok(())
            }
            _ => {
                lists.cache().evict_one();
                Ok(())
            }
        };
        self.expected_failure(ctx, result)
    }

    /// One to three requests in one batch, each with one to four operations; each request checks that it reads
    /// the writes of the requests before it, and a request that does not is returned as the violation's detail.
    fn commit(
        &mut self,
        ctx: &mut SimContext,
        lists: &TestLists,
    ) -> Result<Option<String>, PostingError> {
        let mut batch = lists.batch();
        let mut written: Vec<BTreeMap<Vec<u8>, Vec<Op>>> = Vec::new();
        for _ in 0..ctx.rng().between(1, 3) {
            let read_ts = lists.fresh_ts();
            let mut request = batch.request(read_ts);
            let mut ops: BTreeMap<Vec<u8>, Vec<Op>> = BTreeMap::new();
            for _ in 0..ctx.rng().between(1, 4) {
                let key = self.key(ctx);
                let op = self.op(ctx, &key);
                let blind = ctx.rng().chance(300);
                let list = if blind {
                    request.get_from_delta(&key)
                } else {
                    request.get(&key)?
                };
                list.add_posting(op.posting(), false);
                ops.entry(key).or_default().push(op);
            }
            for (key, own) in &ops {
                let mut expected = self.model.held(key, read_ts);
                written
                    .iter()
                    .filter_map(|w| w.get(key))
                    .chain(std::iter::once(own))
                    .flatten()
                    .for_each(|op| expected.apply(op));
                let seen = Held::of(request.get(key)?, read_ts)?;
                if seen != expected {
                    return Ok(Some(format!(
                        "key {} at {read_ts} read {seen:?} in its batch, expected {expected:?}",
                        hex(key)
                    )));
                }
            }
            batch.push(request);
            written.push(ops);
        }
        let first = lists.peek_ts();
        match lists.commit(batch) {
            Ok(timestamps) => {
                for (ts, ops) in timestamps.into_iter().zip(written) {
                    for (key, ops) in ops {
                        self.model.commit(&key, ts, ops);
                    }
                }
                Ok(None)
            }
            Err(error) => {
                self.pending = Some((first, written));
                Err(error)
            }
        }
    }

    fn op(&self, ctx: &mut SimContext, key: &[u8]) -> Op {
        let roll = ctx.rng().below(100);
        if self.is_uid_key(key) {
            let uid = ctx.rng().between(1, UIDS);
            match roll {
                0..70 => Op::Set(uid),
                70..95 => Op::Del(uid),
                _ => Op::DeleteAll,
            }
        } else {
            let value = format!("v{}", ctx.rng().below(5)).into_bytes();
            match roll {
                0..70 => Op::SetValue(value),
                70..90 => Op::DelValue(value),
                _ => Op::DeleteAll,
            }
        }
    }

    /// One read: a key at a timestamp from below the discard timestamp to beyond the newest commit, through the
    /// cache and around it, each compared with the model.
    fn read(&mut self, ctx: &mut SimContext) -> Result<(), Violation> {
        let lists = self.lists(ctx);
        let key = self.key(ctx);
        let discard = lists.store().discard_ts();
        let read_ts = ctx
            .rng()
            .between(discard.saturating_sub(1).max(1), lists.applied_ts() + 2);
        let expected = self.model.held(&key, read_ts);
        let uncached = lists
            .read_uncached(&key, read_ts)
            .and_then(|l| Held::of(&l, read_ts));
        let cached = lists
            .read(&key, read_ts)
            .and_then(|l| Held::of(&l, read_ts));
        for (result, rule) in [
            (uncached.clone(), READ_MATCHES_MODEL),
            (cached.clone(), CACHE_MATCHES_STORE),
        ] {
            match result {
                Ok(held) if held == expected => self.reads += 1,
                Ok(held) => {
                    return Err(ctx.violation(
                        rule,
                        format!(
                            "key {} at {read_ts}: read {held:?}, the model holds {expected:?}",
                            hex(&key)
                        ),
                    ));
                }
                Err(error) => self.expected_failure(ctx, Err(error))?,
            }
        }
        Ok(())
    }

    /// Accepts the errors the simulator's faults explain: a failed or refused write, a failed read, a read below
    /// the discard timestamp, a store waiting for recovery.
    fn expected_failure(
        &self,
        ctx: &SimContext,
        result: Result<(), PostingError>,
    ) -> Result<(), Violation> {
        match result {
            Ok(()) => Ok(()),
            Err(PostingError(PostingErrorEnum::Store(StoreError(
                StoreErrorEnum::WriteFailed(_)
                | StoreErrorEnum::Poisoned
                | StoreErrorEnum::ReadFailed { .. }
                | StoreErrorEnum::TsTooOld,
            )))) => Ok(()),
            Err(error) => Err(ctx.violation(NO_UNEXPECTED_ERROR, error.to_string())),
        }
    }
}

fn hex(key: &[u8]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}
