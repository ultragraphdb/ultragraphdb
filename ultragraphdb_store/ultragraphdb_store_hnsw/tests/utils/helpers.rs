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
//! use, and inserts, deletes and searches over the posting lists of a store, as index maintenance makes them: a
//! node's vector in its forward list as a `ugdb:float32vector` literal's encoding, the index's lists in the
//! vector-index keyspace.

use std::cell::RefCell;
use std::collections::BTreeMap;

use oxrdf::{Literal, NamedNode};
use ultragraphdb_core_keys::DEFAULT_GRAPH;
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, TermError, format_vector};
use ultragraphdb_core_tok::{VEC_DEAD, VEC_ENTRY, VEC_KEYWORD, get_index_factory};
use ultragraphdb_proto_pb::pb::{Posting, posting};
use ultragraphdb_store_hnsw::{
    HnswError, HnswFloat, PersistentHnsw, QueryCache, TxnCache, VectorKeys, ViTxn, add_dead_node,
    index_posting,
};
use ultragraphdb_store_posting::{CachedList, DEL, Lists, ListsConfig, PostingError, Request, SET};
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

/// The vector predicate `name` in the root namespace, as a namespaced attribute.
pub fn pred(name: &str) -> String {
    format!("0-{name}")
}

/// The keys of the index of the vector predicate `name`, a namespaced attribute as Dgraph's tests name one, in the
/// default graph of the root namespace: its UID drawn from its name.
pub fn vkeys(name: &str) -> VectorKeys {
    let uid = name
        .bytes()
        .fold(1u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)))
        .max(1);
    VectorKeys::new(name, 0, DEFAULT_GRAPH, uid)
}

/// The quad key Dgraph's key of `attr` at `uid` translates to: `<pred>` a node's forward list, `<pred>__vector_`
/// its neighbour lists, `<pred>__vector_entry` and `<pred>__vector_dead` the index's entry point and dead list, the
/// predicate's [`vkeys`].
pub fn key(attr: &str, uid: u64) -> Vec<u8> {
    if let Some(name) = attr.strip_suffix(VEC_ENTRY) {
        return vkeys(name).entry();
    }
    if let Some(name) = attr.strip_suffix(VEC_DEAD) {
        return vkeys(name).dead();
    }
    if let Some(name) = attr.strip_suffix(VEC_KEYWORD) {
        return vkeys(name).neighbours(uid);
    }
    vkeys(attr).vector(uid)
}

/// The quad key a data key of Dgraph's layout, as the golden files hold one, translates to: its namespace and
/// attribute as a namespaced attribute, and its uid, through [`key`]. `None` for bytes of another layout.
pub fn translated(dgraph_key: &[u8]) -> Option<Vec<u8>> {
    let ns = u64::from_be_bytes(dgraph_key.get(1..9)?.try_into().ok()?);
    let len = usize::from(u16::from_be_bytes(dgraph_key.get(9..11)?.try_into().ok()?));
    let attr = std::str::from_utf8(dgraph_key.get(11..11 + len)?).ok()?;
    let uid = u64::from_be_bytes(dgraph_key.get(12 + len..20 + len)?.try_into().ok()?);
    Some(key(&format!("{ns:x}-{attr}"), uid))
}

/// The encoding of the `ugdb:float32vector` literal holding `v`, what a node's forward list holds.
pub fn vector_encoding(v: &[f32]) -> Vec<u8> {
    let literal = Literal::new_typed_literal(
        format_vector(v.iter().copied()),
        NamedNode::new_unchecked(FLOAT32_VECTOR),
    );
    EncodedTerm::literal::<TermError>(literal.as_ref(), |_| unreachable!("a known datatype"))
        .expect("a canonical vector")
        .into_term()
        .as_bytes()
        .to_vec()
}

/// The posting of the vector `encoding` in a forward list, with `op`.
fn vector_posting(encoding: Vec<u8>, op: u32) -> Posting {
    Posting {
        uid: ultragraphdb_common_utils::go_farm::fingerprint64(&encoding),
        value: encoding,
        posting_type: posting::PostingType::Value as i32,
        op,
        ..Posting::default()
    }
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

/// Sets `uid`'s vector to `v` in `request`, as a write of a `ugdb:float32vector` literal does: the vector it held
/// replaced.
pub fn set_vector<S, C>(
    request: &mut Request<'_, S, C>,
    pred: &str,
    uid: u64,
    v: &[f32],
) -> Result<(), PostingError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let keys = vkeys(pred);
    let read_ts = request.read_ts();
    let list = request.get(&keys.vector(uid))?;
    for held in held_values(list, read_ts)? {
        list.add_posting(vector_posting(held, DEL), false);
    }
    list.add_posting(vector_posting(vector_encoding(v), SET), false);
    Ok(())
}

/// The values `list` holds at `read_ts`.
fn held_values(
    list: &ultragraphdb_store_posting::List,
    read_ts: u64,
) -> Result<Vec<Vec<u8>>, PostingError> {
    let mut held = Vec::new();
    list.iterate(read_ts, 0, |p| {
        held.push(p.value.clone());
        Ok::<_, PostingError>(std::ops::ControlFlow::Continue(()))
    })?;
    Ok(held)
}

/// Deletes `uid`'s vector in `request` and appends `uid` to the dead list, as index maintenance does for a deleted
/// vector.
pub fn delete_vector<S, C>(
    request: &mut Request<'_, S, C>,
    pred: &str,
    uid: u64,
) -> Result<(), HnswError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let keys = vkeys(pred);
    let read_ts = request.read_ts();
    let list = request.get(&keys.vector(uid))?;
    for held in held_values(list, read_ts)? {
        list.add_posting(vector_posting(held, DEL), false);
    }
    let dead = request.get(&keys.dead())?;
    let existing = dead
        .static_value(read_ts)
        .and_then(|pl| pl.postings.first())
        .filter(|p| p.op != DEL)
        .map(|p| p.value.clone());
    dead.add_posting(
        index_posting(add_dead_node(existing.as_deref(), uid)?),
        false,
    );
    Ok(())
}

/// Sets `uid`'s vector and inserts it into the index of `pred` built from `o`, in one request committed on its own:
/// the values the insert wrote.
pub fn insert<S>(
    lists: &Lists<S, SimListCache<CachedList>>,
    pred: &str,
    o: &Options,
    uid: u64,
    vec: &[f32],
    draw: &mut impl FnMut() -> f64,
) -> Result<Vec<KeyValue>, HnswError>
where
    S: Store,
{
    let read_ts = lists.fresh_ts();
    let mut batch = lists.batch();
    let mut request = batch.request(read_ts);
    set_vector(&mut request, pred, uid, vec)?;
    let edges = {
        let txn = ViTxn::new(&mut request);
        let index = PersistentHnsw::<f32>::new(vkeys(pred), o, 32)?;
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
pub fn search<S>(
    lists: &Lists<S, SimListCache<CachedList>>,
    pred: &str,
    o: &Options,
    query: &[f32],
    k: usize,
    options: Option<&VectorIndexOptions<f32>>,
) -> Result<Vec<u64>, HnswError>
where
    S: Store,
{
    let read_ts = lists.latest_ts();
    let mut request = lists.request(read_ts);
    let txn = ViTxn::new(&mut request);
    let qc = QueryCache::new(&txn, read_ts);
    let index = PersistentHnsw::<f32>::new(vkeys(pred), o, 32)?;
    match options {
        Some(options) => index.search_with_options(&qc, query, k, options),
        None => index.search(&qc, query, k, accept_all::<f32>),
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
/// `NoValue`; `find` scans the forward keys under the prefix in key order and names the first passing key's node, or
/// reports `KeyNotFound`. A node's vector is held raw, as Go's golden transaction holds it.
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
        let db = self.db.borrow();
        for (k, v) in db.range(prefix.to_vec()..) {
            if !k.starts_with(prefix) {
                break;
            }
            if v.is_empty() {
                continue;
            }
            if filter(v) {
                return match ultragraphdb_core_keys::QuadKey::parse(k) {
                    Ok((ultragraphdb_core_keys::QuadKey::Forward { subject, .. }, _)) => {
                        Ok(subject)
                    }
                    _ => Err(HnswError::KeyNotFound()),
                };
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
