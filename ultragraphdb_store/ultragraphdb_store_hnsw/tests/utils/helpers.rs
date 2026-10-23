/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/test_helper.go (indexStorage, inMemList, inMemTxn, inMemLocalCache,
 * floatArrayAsBytes) and ef_recall_test.go (memoryCache, float64ArrayAsBytes). Changed for ultragraphdb: translated
 * to Rust; the hundred timestamped maps belong to a value, not to a global; one thread uses them, so their locks are
 * not ported; an absent key is the index's absence, `NoValue`, where Dgraph's doubles return an error of their own
 * that Dgraph's index takes for absence; the maps iterate in key order, where Go's iterate at random. Added: the
 * golden runs' transaction and the index's operations over the posting crate's lists, as index maintenance makes
 * them.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! What the unit tests and the scenarios share: the in-memory doubles Dgraph's tests use, the double the golden runs
//! use, and inserts, deletes and searches over the posting lists of a store, as index maintenance makes them.

use std::cell::RefCell;
use std::collections::BTreeMap;

use ultragraphdb_core_keys::{attr_in_root_namespace, data_key, parse};
use ultragraphdb_core_tok::{VEC_DEAD, get_index_factory};
use ultragraphdb_core_types::Value;
use ultragraphdb_proto_pb::pb::{DirectedEdge, directed_edge, posting};
use ultragraphdb_store_hnsw::{
    HnswError, HnswFloat, PersistentHnsw, QueryCache, TxnCache, ViTxn, add_dead_node,
};
use ultragraphdb_store_posting::{
    CachedList, Lists, ListsConfig, PostingError, PostingErrorEnum, Request,
};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimRng, SimStore, Stream};
use ultragraphdb_traits_index::{
    CacheType, KeyValue, LocalCache, Options, Txn, VectorIndexOptions, accept_all,
};
use ultragraphdb_traits_store::{ListCache, Store};

/// Lists over the simulated store.
pub type TestLists = Lists<SimStore, SimListCache<CachedList>>;

/// A fault-free simulated store.
pub fn new_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

/// Lists over `store` with a cache of 1,024 entries.
pub fn lists_over<S: Store>(store: S) -> Lists<S, SimListCache<CachedList>> {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    Lists::new(store, cache, ListsConfig::default())
}

/// Lists over a fault-free simulated store.
pub fn new_lists() -> TestLists {
    lists_over(new_store())
}

/// The vector predicate `name` in the root namespace.
pub fn pred(name: &str) -> String {
    attr_in_root_namespace(name)
}

/// The data key of `attr` for `uid`.
pub fn key(attr: &str, uid: u64) -> Vec<u8> {
    data_key(attr, uid).expect("a data key")
}

/// Options parsed as the hnsw factory parses a schema's: `exponent`, `maxLevels`, `efConstruction`, `efSearch` and
/// `metric`.
pub fn opts(pairs: &[(&str, &str)]) -> Options {
    let allowed = get_index_factory("hnsw")
        .expect("the hnsw factory")
        .allowed_options();
    let mut o = Options::new();
    for (k, v) in pairs {
        o.set_opt(*k, allowed.get_parsed_option(k, v).expect("a valid option"));
    }
    o
}

/// Go's `floatArrayAsBytes`: float64s, little-endian.
pub fn float64_array_as_bytes(v: &[f64]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// A vector's bytes, little-endian at its width.
pub fn vec_bytes<T: HnswFloat>(v: &[T]) -> Vec<u8> {
    v.iter()
        .flat_map(|f| {
            if T::BITS == 32 {
                (f.to_f64() as f32).to_le_bytes().to_vec()
            } else {
                f.to_f64().to_le_bytes().to_vec()
            }
        })
        .collect()
}

/// A uniform draw in [0, 1) from `rng`, as Go's `Float64` draws one from 53 bits.
pub fn draw(rng: &mut SimRng) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

/// The draws `values` give, in order; the draw after the last is 0.
pub fn draws(values: Vec<f64>) -> impl FnMut() -> f64 {
    let mut it = values.into_iter();
    move || it.next().unwrap_or(0.0)
}

/// Sets `uid`'s vector to `bytes` in `request`, as a mutation of a `float32vector` predicate does.
pub fn set_vector<S, C>(
    request: &mut Request<'_, S, C>,
    pred: &str,
    uid: u64,
    bytes: Vec<u8>,
) -> Result<(), PostingError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let edge = DirectedEdge {
        entity: uid,
        attr: pred.to_string(),
        value: bytes,
        value_type: posting::ValType::Vfloat as i32,
        op: directed_edge::Op::Set as i32,
        ..DirectedEdge::default()
    };
    request.get(&key(pred, uid))?.add_mutation(&edge, None)
}

/// Deletes `uid`'s vector in `request` and appends `uid` to the dead list, as `posting/index.go`'s index maintenance
/// does for a deleted vector.
pub fn delete_vector<S, C>(
    request: &mut Request<'_, S, C>,
    pred: &str,
    uid: u64,
) -> Result<(), HnswError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let read_ts = request.read_ts();
    let data = request.get(&key(pred, uid))?;
    let stored = value_bytes(data.value(read_ts))?;
    let edge = DirectedEdge {
        entity: uid,
        attr: pred.to_string(),
        value: stored.unwrap_or_default(),
        value_type: posting::ValType::Vfloat as i32,
        op: directed_edge::Op::Del as i32,
        ..DirectedEdge::default()
    };
    data.add_mutation(&edge, None)?;
    let dead_attr = format!("{pred}{VEC_DEAD}");
    let dead_key = key(&dead_attr, 1);
    let dead = request.get(&dead_key)?;
    let existing = value_bytes(dead.value(read_ts))?;
    let dead_edge = DirectedEdge {
        entity: 1,
        attr: dead_attr,
        value: add_dead_node(existing.as_deref(), uid)?,
        value_type: 0,
        op: directed_edge::Op::Set as i32,
        ..DirectedEdge::default()
    };
    dead.add_mutation(&dead_edge, None)?;
    Ok(())
}

/// A value's bytes, `None` for no value.
fn value_bytes(
    value: Result<ultragraphdb_core_types::Val, PostingError>,
) -> Result<Option<Vec<u8>>, PostingError> {
    match value {
        Ok(v) => Ok(match v.into_value() {
            Some(Value::Bytes(b)) => Some(b),
            _ => None,
        }),
        Err(PostingError(PostingErrorEnum::NoValue)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Sets `uid`'s vector and inserts it into the index of `pred` built from `o`, in one request committed on its own:
/// the values the insert wrote.
pub fn insert<S, T>(
    lists: &Lists<S, SimListCache<CachedList>>,
    pred: &str,
    o: &Options,
    uid: u64,
    vec: &[T],
    draw: &mut impl FnMut() -> f64,
) -> Result<Vec<KeyValue>, HnswError>
where
    S: Store,
    T: HnswFloat,
{
    let read_ts = lists.fresh_ts();
    let mut batch = lists.batch();
    let mut request = batch.request(read_ts);
    set_vector(&mut request, pred, uid, vec_bytes(vec))?;
    let edges = {
        let txn = ViTxn::new(&mut request);
        let index = PersistentHnsw::<T>::new(pred, o, T::BITS)?;
        index.insert(&TxnCache::new(&txn, read_ts), uid, vec, draw)?
    };
    batch.push(request);
    lists.commit(batch)?;
    Ok(edges)
}

/// Deletes `uid`'s vector of `pred` in one request committed on its own.
pub fn delete<S>(
    lists: &Lists<S, SimListCache<CachedList>>,
    pred: &str,
    uid: u64,
) -> Result<(), HnswError>
where
    S: Store,
{
    let read_ts = lists.fresh_ts();
    let mut batch = lists.batch();
    let mut request = batch.request(read_ts);
    delete_vector(&mut request, pred, uid)?;
    batch.push(request);
    lists.commit(batch)?;
    Ok(())
}

/// Searches the index of `pred` for the `k` nearest to `query` at the newest timestamp, with `options`.
pub fn search<S, T>(
    lists: &Lists<S, SimListCache<CachedList>>,
    pred: &str,
    o: &Options,
    query: &[T],
    k: usize,
    options: Option<&VectorIndexOptions<T>>,
) -> Result<Vec<u64>, HnswError>
where
    S: Store,
    T: HnswFloat,
{
    let read_ts = lists.latest_ts();
    let mut request = lists.request(read_ts);
    let txn = ViTxn::new(&mut request);
    let qc = QueryCache::new(&txn, read_ts);
    let index = PersistentHnsw::<T>::new(pred, o, T::BITS)?;
    match options {
        Some(options) => index.search_with_options(&qc, query, k, options),
        None => index.search(&qc, query, k, accept_all::<T>),
    }
}

/// The value of `key` at `read_ts` as the index reads it, `None` when absent.
pub fn index_value<S: Store>(
    lists: &Lists<S, SimListCache<CachedList>>,
    key: &[u8],
    read_ts: u64,
) -> Option<Vec<u8>> {
    let mut request = lists.request(read_ts);
    let txn = ViTxn::new(&mut request);
    match Txn::get(&txn, key) {
        Ok(v) => Some(v),
        Err(e) if e.is_absence() => None,
        Err(e) => panic!("{e}"),
    }
}

// The doubles of Dgraph's tests.

/// Dgraph's `tsDbs`: the store as a map per timestamp, 0 to 99; a write at `t` is in every map from `t` on.
#[derive(Debug)]
pub struct TsDbs {
    dbs: RefCell<Vec<BTreeMap<Vec<u8>, Vec<u8>>>>,
}

impl Default for TsDbs {
    fn default() -> Self {
        Self {
            dbs: RefCell::new(vec![BTreeMap::new(); 100]),
        }
    }
}

impl TsDbs {
    /// Sets `key` to `value` at every timestamp.
    pub fn set_all(&self, key: &[u8], value: Vec<u8>) {
        self.dbs.borrow_mut().iter_mut().for_each(|db| {
            db.insert(key.to_vec(), value.clone());
        });
    }

    /// The value of `key` at `ts`.
    pub fn get(&self, ts: u64, key: &[u8]) -> Option<Vec<u8>> {
        self.dbs.borrow()[ts as usize].get(key).cloned()
    }

    /// Writes `value` to `key` from `from` on: a commit at `from`.
    pub fn write_from(&self, from: u64, key: &[u8], value: &[u8]) {
        self.dbs.borrow_mut()[from as usize..]
            .iter_mut()
            .for_each(|db| {
                db.insert(key.to_vec(), value.to_vec());
            });
    }

    fn first_passing(&self, ts: u64, filter: impl Fn(&[u8]) -> bool) -> u64 {
        u64::from(self.dbs.borrow()[ts as usize].values().any(|b| filter(b)))
    }
}

/// Dgraph's `inMemList`: a list of `key` written by a transaction from `start_ts` to `finish_ts`.
#[derive(Debug)]
pub struct InMemList<'d> {
    dbs: &'d TsDbs,
    key: Vec<u8>,
    start_ts: u64,
    finish_ts: u64,
}

impl<'d> InMemList<'d> {
    /// Dgraph's `newInMemList`.
    pub fn new(dbs: &'d TsDbs, key: &str, start_ts: u64, finish_ts: u64) -> Self {
        Self {
            dbs,
            key: key.as_bytes().to_vec(),
            start_ts,
            finish_ts,
        }
    }

    /// Dgraph's `AddMutation`: `t`'s value from the commit timestamp on.
    pub fn add_mutation(&self, t: &KeyValue) -> Result<(), HnswError> {
        self.dbs.write_from(self.finish_ts, &self.key, t.value());
        Ok(())
    }

    pub fn start_ts(&self) -> u64 {
        self.start_ts
    }
}

/// Dgraph's `inMemTxn`: reads at `start_ts`, writes from `commit_ts` on.
#[derive(Debug)]
pub struct InMemTxn<'d> {
    dbs: &'d TsDbs,
    start_ts: u64,
    commit_ts: u64,
}

impl<'d> InMemTxn<'d> {
    pub fn new(dbs: &'d TsDbs, start_ts: u64, commit_ts: u64) -> Self {
        Self {
            dbs,
            start_ts,
            commit_ts,
        }
    }
}

impl Txn for InMemTxn<'_> {
    type Error = HnswError;

    fn start_ts(&self) -> u64 {
        self.start_ts
    }

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.dbs
            .get(self.start_ts, key)
            .ok_or_else(HnswError::NoValue)
    }

    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Txn::get(self, key)
    }

    /// Dgraph's double: 1 when any value passes, whatever its key.
    fn find(&self, _: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        Ok(self.dbs.first_passing(self.start_ts, filter))
    }

    fn add_mutation(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        self.add_mutation_with_lock_held(key, t)
    }

    fn add_mutation_with_lock_held(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        self.dbs.write_from(self.commit_ts, key, t.value());
        Ok(())
    }

    fn lock_key(&self, _: &[u8]) {}

    fn unlock_key(&self, _: &[u8]) {}
}

/// Dgraph's `inMemLocalCache`: reads at `read_ts`.
#[derive(Debug)]
pub struct InMemLocalCache<'d> {
    dbs: &'d TsDbs,
    read_ts: u64,
}

impl<'d> InMemLocalCache<'d> {
    pub fn new(dbs: &'d TsDbs, read_ts: u64) -> Self {
        Self { dbs, read_ts }
    }
}

impl LocalCache for InMemLocalCache<'_> {
    type Error = HnswError;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.dbs
            .get(self.read_ts, key)
            .ok_or_else(HnswError::NoValue)
    }

    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        LocalCache::get(self, key)
    }

    fn find(&self, _: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        Ok(self.dbs.first_passing(self.read_ts, filter))
    }
}

/// `ef_recall_test.go`'s `memoryCache`: an absent key reads as no bytes and no error, and `find` finds nothing.
#[derive(Debug, Default)]
pub struct MemoryCache {
    pub data: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl CacheType for MemoryCache {
    type Error = HnswError;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Ok(self.data.get(key).cloned().unwrap_or_default())
    }

    fn ts(&self) -> u64 {
        0
    }

    fn find(&self, _: &[u8], _: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        Ok(0)
    }
}

/// The golden runs' transaction, `tools/golden/hnsw.go`'s `goldenTxn`: one map; an absent or empty value is
/// `NoValue`; `find` scans the predicate's data keys in key order and names the first passing key's uid, or reports
/// `KeyNotFound`.
#[derive(Debug, Default)]
pub struct MapTxn {
    pub db: RefCell<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl MapTxn {
    fn read(&self, key: &[u8]) -> Result<Vec<u8>, HnswError> {
        match self.db.borrow().get(key) {
            Some(v) if !v.is_empty() => Ok(v.clone()),
            _ => Err(HnswError::NoValue()),
        }
    }

    fn scan(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, HnswError> {
        let attr = String::from_utf8_lossy(prefix);
        let start = key(&attr, 0);
        let pfx = &start[..start.len() - 8];
        let db = self.db.borrow();
        for (k, v) in db.range(pfx.to_vec()..) {
            if !k.starts_with(pfx) {
                break;
            }
            if v.is_empty() {
                continue;
            }
            if filter(v) {
                return Ok(parse(k)?.uid());
            }
        }
        Err(HnswError::KeyNotFound())
    }
}

impl Txn for MapTxn {
    type Error = HnswError;

    fn start_ts(&self) -> u64 {
        1
    }

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.read(key)
    }

    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.read(key)
    }

    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        self.scan(prefix, filter)
    }

    fn add_mutation(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        self.add_mutation_with_lock_held(key, t)
    }

    fn add_mutation_with_lock_held(&self, key: &[u8], t: &KeyValue) -> Result<(), Self::Error> {
        self.db
            .borrow_mut()
            .insert(key.to_vec(), t.value().to_vec());
        Ok(())
    }

    fn lock_key(&self, _: &[u8]) {}

    fn unlock_key(&self, _: &[u8]) {}
}

impl LocalCache for MapTxn {
    type Error = HnswError;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.read(key)
    }

    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.read(key)
    }

    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        self.scan(prefix, filter)
    }
}

/// The edges' names as Dgraph's tests print them, `attr_entity`.
pub fn edge_names(edges: &[KeyValue]) -> Vec<String> {
    edges
        .iter()
        .map(|e| format!("{}_{}", e.attr(), e.entity()))
        .collect()
}
