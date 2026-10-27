/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The vector index's workload on the storage simulator, with its shadow model. The apply loop commits batches of one
//! or two requests, each inserting a new vector into the index or deleting one (its value and its uid appended to
//! the dead list, as index maintenance does for a star delete), syncs, moves the discard timestamp and compacts,
//! rolls lists up, advances the cache's epoch and evicts cache entries; readers search at timestamps from the discard timestamp to
//! beyond the newest commit, and check the stored graph. The model holds every vector's insert and delete timestamps.
//!
//! The rules:
//!
//! * a search returns only vectors live at its timestamp, each once, at most `k`, best first by the metric;
//! * a search reports the empty index exactly when no vector is live;
//! * every uid a stored neighbour list or the entry point names has stored neighbour lists, one per level;
//! * the neighbour lists an insert writes name no node dead at its read timestamp.
//!
//! The simulator's own model checks the store underneath.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ultragraphdb_core_tok::{VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_hnsw::{
    HnswError, HnswErrorEnum, PersistentHnsw, SimilarityType, TxnCache, ViTxn, bytes_to_uint64,
    decode_uint64_matrix,
};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig, PostingError, PostingErrorEnum};
use ultragraphdb_store_sim::{
    Event, FaultConfig, RunOutcome, Seed, SimContext, SimListCache, SimRng, Simulation, Stream,
    Violation,
};
use ultragraphdb_traits_index::{KeyValue, Metric, Options};
use ultragraphdb_traits_store::{PersistMode, Scan, Store, StoreSnapshot};

use ultragraphdb_core_keys::QuadKey;

use crate::utils::helpers::{
    TestLists, delete_vector, draw, key, opts, oracle_score, pred, set_vector, vkeys,
};

/// The rule a search breaks when it returns a vector that is not live, twice, too many, or out of order.
pub const SEARCH_FINDS_LIVE: &str =
    "a search returns only vectors live at its timestamp, each once, at most k, best first";
/// The rule broken when a search reports the empty index while a vector is live, or not while none is.
pub const EMPTY_ONLY_WHEN_EMPTY: &str =
    "a search reports the empty index exactly when no vector is live";
/// The rule broken by a neighbour list or entry point naming a node without stored neighbour lists.
pub const GRAPH_CLOSED: &str = "every uid a stored neighbour list or the entry point names has stored neighbour lists, one per level";
/// The rule broken by an insert writing a neighbour list that names a dead node.
pub const DEAD_LEFT_OUT: &str =
    "the neighbour lists an insert writes name no node dead at its read timestamp";
/// The rule broken by an error no fault explains.
pub const NO_UNEXPECTED_ERROR: &str =
    "a vector-index operation fails only for a fault the simulator injected";

const LEVELS: usize = 3;
const DIMS: usize = 3;

/// A vector's life in the model.
#[derive(Debug, Clone)]
struct Life {
    vector: Vec<f32>,
    inserted: u64,
    deleted: Option<u64>,
}

/// One request's operation.
#[derive(Debug, Clone)]
enum Op {
    Insert(u64, Vec<f32>),
    Delete(u64),
}

/// The shadow model: every vector's insert and delete timestamps.
#[derive(Debug, Clone, Default)]
pub struct Model {
    lives: BTreeMap<u64, Life>,
}

impl Model {
    fn live(&self, uid: u64, ts: u64) -> Option<&Vec<f32>> {
        self.lives
            .get(&uid)
            .filter(|l| l.inserted <= ts && l.deleted.is_none_or(|d| d > ts))
            .map(|l| &l.vector)
    }

    fn live_uids(&self, ts: u64) -> Vec<u64> {
        self.lives
            .keys()
            .copied()
            .filter(|&u| self.live(u, ts).is_some())
            .collect()
    }

    fn dead(&self, ts: u64) -> BTreeSet<u64> {
        self.lives
            .iter()
            .filter(|(_, l)| l.deleted.is_some_and(|d| d <= ts))
            .map(|(&u, _)| u)
            .collect()
    }

    fn commit(&mut self, ts: u64, op: &Op) {
        match op {
            Op::Insert(uid, v) => {
                self.lives.insert(
                    *uid,
                    Life {
                        vector: v.clone(),
                        inserted: ts,
                        deleted: None,
                    },
                );
            }
            Op::Delete(uid) => {
                if let Some(l) = self.lives.get_mut(uid) {
                    l.deleted = Some(ts);
                }
            }
        }
    }

    /// Forgets the commits a recovery did not keep.
    fn truncate(&mut self, applied_ts: u64) {
        self.lives.retain(|_, l| l.inserted <= applied_ts);
        for l in self.lives.values_mut() {
            if l.deleted.is_some_and(|d| d > applied_ts) {
                l.deleted = None;
            }
        }
    }
}

/// The vector index's workload of one run.
pub struct HnswWorkload {
    pred: String,
    options: Options,
    metric: Metric,
    lists: Option<Arc<TestLists>>,
    model: Model,
    pending: Option<(u64, Vec<Op>)>,
    next_uid: u64,
    inserts: usize,
    searches: usize,
}

impl HnswWorkload {
    /// The workload of `seed`, its metric drawn from it.
    pub fn new(seed: Seed) -> Self {
        let metric = ["euclidean", "cosine", "dotproduct"][(seed.value() % 3) as usize];
        let options = opts(&[
            ("maxLevels", "3"),
            ("efConstruction", "8"),
            ("efSearch", "6"),
            ("metric", metric),
        ]);
        Self {
            pred: pred("emb"),
            options,
            metric: SimilarityType::from_name(metric).metric(),
            lists: None,
            model: Model::default(),
            pending: None,
            next_uid: 1,
            inserts: 0,
            searches: 0,
        }
    }

    /// The faults of a vector-index run: every class but corruption, which a neighbour matrix that still decodes
    /// leaves undetectable.
    pub fn faults() -> FaultConfig {
        FaultConfig::mixed().with_corruption(0)
    }

    /// Runs the workload with `readers` readers for `steps` steps of `seed`: the outcome, and how many inserts and
    /// searches it checked.
    pub fn run(
        seed: Seed,
        faults: FaultConfig,
        steps: u64,
        readers: usize,
    ) -> Result<(RunOutcome, usize, usize), Violation> {
        let mut workload = Self::new(seed);
        let outcome = Simulation::new(seed, faults)
            .run(steps, readers + 1, |ctx, event| workload.step(ctx, event))?;
        Ok((outcome, workload.inserts, workload.searches))
    }

    fn lists(&mut self, ctx: &SimContext) -> Arc<TestLists> {
        self.lists
            .get_or_insert_with(|| {
                let cache = SimListCache::new(8, ctx.seed().stream(Stream::Cache));
                Arc::new(Lists::new(
                    ctx.store().clone(),
                    cache,
                    ListsConfig::new(PersistMode::Buffer, 1 << 20),
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

    fn recovered(&mut self, ctx: &SimContext) {
        self.lists = None;
        let applied = ctx.store().applied_ts();
        if let Some((first, ops)) = self.pending.take() {
            for (i, op) in ops.iter().enumerate() {
                self.model.commit(first + 2 * i as u64, op);
            }
        }
        self.model.truncate(applied);
    }

    fn write(&mut self, ctx: &mut SimContext) -> Result<(), Violation> {
        let lists = self.lists(ctx);
        let result = match ctx.rng().below(100) {
            0..70 => match self.commit(ctx, &lists) {
                Ok(Some(detail)) => return Err(ctx.violation(DEAD_LEFT_OUT, detail)),
                Ok(None) => Ok(()),
                Err(e) => Err(e),
            },
            70..76 => lists.sync().map_err(HnswError::from),
            76..84 => {
                let applied = lists.applied_ts();
                let discard = ctx.rng().between(lists.store().discard_ts(), applied);
                lists
                    .set_discard_ts(discard)
                    .and_then(|()| Ok(lists.store().compact()?))
                    .map_err(HnswError::from)
            }
            84..88 => {
                lists.spawn_rollups(ctx.executor(), ctx.step());
                Ok(())
            }
            88..92 => {
                let uid = ctx.rng().between(1, self.next_uid);
                let attr = if ctx.rng().chance(500) {
                    format!("{}{VEC_KEYWORD}", self.pred)
                } else {
                    self.pred.clone()
                };
                lists
                    .rollup_key(&key(&attr, uid))
                    .map(|_| ())
                    .map_err(HnswError::from)
            }
            92..96 => {
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

    /// One or two requests in one batch, each inserting a new vector or deleting a live one. Returns the detail of a
    /// written neighbour list naming a dead node.
    fn commit(
        &mut self,
        ctx: &mut SimContext,
        lists: &TestLists,
    ) -> Result<Option<String>, HnswError> {
        let mut batch = lists.batch();
        let mut ops = Vec::new();
        let mut batch_dead = BTreeSet::new();
        for _ in 0..ctx.rng().between(1, 2) {
            let read_ts = lists.applied_ts();
            let mut request = batch.request(read_ts);
            let live = self.model.live_uids(read_ts);
            let delete = !live.is_empty() && ctx.rng().chance(250);
            if delete {
                let uid = live[ctx.rng().index(live.len())];
                if batch_dead.contains(&uid) {
                    continue;
                }
                delete_vector(&mut request, &self.pred, uid)?;
                batch_dead.insert(uid);
                ops.push(Op::Delete(uid));
            } else {
                let uid = self.next_uid;
                self.next_uid += 1;
                let v = random_vector(ctx.rng());
                set_vector(&mut request, &self.pred, uid, &v)?;
                let edges = {
                    let txn = ViTxn::new(&mut request);
                    let index = PersistentHnsw::<f32>::new(vkeys(&self.pred), &self.options, 32)?;
                    let rng = ctx.rng();
                    index.insert(&TxnCache::new(&txn, read_ts), uid, &v, &mut || draw(rng))?
                };
                let mut dead = self.model.dead(read_ts);
                dead.extend(batch_dead.iter().copied());
                if let Some(detail) =
                    names_dead(&edges, &format!("{}{VEC_KEYWORD}", self.pred), &dead)
                {
                    return Ok(Some(format!("insert of {uid} at {read_ts}: {detail}")));
                }
                self.inserts += 1;
                ops.push(Op::Insert(uid, v));
            }
            batch.push(request);
        }
        let first = lists.next_version();
        match lists.commit(batch) {
            Ok(timestamps) => {
                for (ts, op) in timestamps.into_iter().zip(&ops) {
                    self.model.commit(ts, op);
                }
                Ok(None)
            }
            Err(e) => {
                self.pending = Some((first, ops));
                Err(e.into())
            }
        }
    }

    /// One search at a timestamp from below the discard timestamp to the watermark, compared with the
    /// model; one time in four, the stored graph checked at that timestamp too.
    fn read(&mut self, ctx: &mut SimContext) -> Result<(), Violation> {
        let lists = self.lists(ctx);
        let discard = lists.store().discard_ts();
        let read_ts = ctx.rng().between(
            discard.saturating_sub(1).min(lists.applied_ts()),
            lists.applied_ts(),
        );
        let query = random_vector(ctx.rng());
        let k = ctx.rng().between(1, 5) as usize;
        let result = {
            let mut request = lists.request(read_ts);
            let txn = ViTxn::new(&mut request);
            let qc = ultragraphdb_store_hnsw::QueryCache::new(&txn, read_ts);
            PersistentHnsw::<f32>::new(vkeys(&self.pred), &self.options, 32).and_then(|index| {
                index.search(&qc, &query, k, ultragraphdb_traits_index::accept_all::<f32>)
            })
        };
        let live = self.model.live_uids(read_ts);
        match result {
            Err(e) if e.is_empty_index() => {
                if !live.is_empty() {
                    return Err(ctx.violation(
                        EMPTY_ONLY_WHEN_EMPTY,
                        format!("at {read_ts} the empty index, with {} live", live.len()),
                    ));
                }
            }
            Ok(uids) => {
                if live.is_empty() && !uids.is_empty() {
                    return Err(ctx.violation(
                        EMPTY_ONLY_WHEN_EMPTY,
                        format!("at {read_ts} found {uids:?}, none live"),
                    ));
                }
                if let Some(detail) = self.check_result(&uids, &query, k, read_ts) {
                    return Err(ctx.violation(SEARCH_FINDS_LIVE, format!("at {read_ts}: {detail}")));
                }
                self.searches += 1;
            }
            Err(e) => self.expected_failure(ctx, Err(e))?,
        }
        if ctx.rng().chance(250) {
            match graph_defect(&lists, &self.pred, read_ts) {
                Ok(Some(detail)) => {
                    return Err(ctx.violation(GRAPH_CLOSED, format!("at {read_ts}: {detail}")));
                }
                Ok(None) => {}
                Err(e) => self.expected_failure(ctx, Err(e))?,
            }
        }
        Ok(())
    }

    fn check_result(&self, uids: &[u64], query: &[f32], k: usize, ts: u64) -> Option<String> {
        if uids.len() > k {
            return Some(format!("{uids:?} for k {k}"));
        }
        if uids.iter().collect::<BTreeSet<_>>().len() != uids.len() {
            return Some(format!("{uids:?} repeats a uid"));
        }
        // The order by the oracle's scores, computed naively in float64, not by the index's kernels.
        let mut scores = Vec::new();
        for uid in uids {
            let Some(v) = self.model.live(*uid, ts) else {
                return Some(format!("{uids:?} holds {uid}, not live"));
            };
            scores.push(oracle_score(self.metric, &widen(v), &widen(query)));
        }
        let higher_first = self.metric != Metric::Euclidean;
        scores
            .windows(2)
            .any(|w| {
                // A float32 tie may differ in float64's last bits.
                let slack = 1e-6 * w[0].abs().max(w[1].abs()).max(1.0);
                if higher_first {
                    w[1] > w[0] + slack
                } else {
                    w[1] < w[0] - slack
                }
            })
            .then(|| format!("{uids:?} out of order: {scores:?}"))
    }

    /// Accepts the errors the simulator's faults explain: a failed or refused write, a failed read, a read below the
    /// discard timestamp, a store waiting for recovery.
    fn expected_failure(
        &self,
        ctx: &SimContext,
        result: Result<(), HnswError>,
    ) -> Result<(), Violation> {
        match result {
            Ok(()) => Ok(()),
            Err(HnswError(HnswErrorEnum::Posting(PostingError(PostingErrorEnum::Store(
                StoreError(
                    StoreErrorEnum::WriteFailed(_)
                    | StoreErrorEnum::Poisoned
                    | StoreErrorEnum::ReadFailed { .. }
                    | StoreErrorEnum::TsTooOld,
                ),
            ))))) => Ok(()),
            Err(e) => Err(ctx.violation(NO_UNEXPECTED_ERROR, e.to_string())),
        }
    }
}

fn widen(v: &[f32]) -> Vec<f64> {
    v.iter().map(|&f| f64::from(f)).collect()
}

fn random_vector(rng: &mut SimRng) -> Vec<f32> {
    (0..DIMS).map(|_| (draw(rng) * 2.0 - 1.0) as f32).collect()
}

/// The first neighbour list among `edges` naming a uid of `dead`.
pub fn names_dead(edges: &[KeyValue], vec_key: &str, dead: &BTreeSet<u64>) -> Option<String> {
    edges.iter().filter(|e| e.attr() == vec_key).find_map(|e| {
        let lists = decode_uint64_matrix(e.value()).ok()?;
        lists
            .iter()
            .flatten()
            .find(|uid| dead.contains(uid))
            .map(|uid| format!("the lists of {} name dead {uid}", e.entity()))
    })
}

/// A defect of the graph stored for `pred` at `read_ts`: a uid named by a neighbour list or the entry point without
/// stored neighbour lists, or lists of other than [`LEVELS`] levels.
pub fn graph_defect<S: Store>(
    lists: &Lists<S, SimListCache<CachedList>>,
    pred: &str,
    read_ts: u64,
) -> Result<Option<String>, HnswError> {
    let mut prefix = vkeys(pred).neighbours(1);
    prefix.truncate(prefix.len() - 8);
    lists.gate(read_ts)?;
    let snapshot = lists
        .store()
        .snapshot(read_ts)
        .map_err(PostingError::from)?;
    let mut nodes = BTreeMap::new();
    for item in snapshot.scan(&Scan::prefix(prefix)) {
        let item = item.map_err(PostingError::from)?;
        let Ok((QuadKey::VectorIndex { node: uid, .. }, _)) = QuadKey::parse(item.key()) else {
            continue;
        };
        if let Some(v) = read_value(lists, item.key(), read_ts)? {
            let levels =
                decode_uint64_matrix(&v).map_err(|r| HnswError::corrupted(item.key(), r))?;
            nodes.insert(uid, levels);
        }
    }
    if let Some(entry) = read_value(lists, &key(&format!("{pred}{VEC_ENTRY}"), 1), read_ts)? {
        let entry = bytes_to_uint64(&entry).unwrap_or_default();
        if !nodes.contains_key(&entry) {
            return Ok(Some(format!("the entry point {entry} has no lists")));
        }
    }
    for (uid, levels) in &nodes {
        if levels.len() != LEVELS {
            return Ok(Some(format!("{uid} has {} levels", levels.len())));
        }
        if let Some(missing) = levels.iter().flatten().find(|n| !nodes.contains_key(n)) {
            return Ok(Some(format!(
                "the lists of {uid} name {missing}, which has none"
            )));
        }
    }
    Ok(None)
}

fn read_value<S: Store>(
    lists: &Lists<S, SimListCache<CachedList>>,
    key: &[u8],
    read_ts: u64,
) -> Result<Option<Vec<u8>>, HnswError> {
    let mut request = lists.request(read_ts);
    let txn = ViTxn::new(&mut request);
    match ultragraphdb_traits_index::Txn::get(&txn, key) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.is_absence() => Ok(None),
        Err(e) => Err(e),
    }
}
