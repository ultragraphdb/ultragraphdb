/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests and the scenarios share: lists over a fault-free simulated store, edges, and the reads and
//! writes the ported Go tests make by hand.

use std::ops::ControlFlow;
use std::sync::{Arc, Mutex};

use ultragraphdb_core_keys::{attr_in_root_namespace, data_key};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_pb::pb::{DirectedEdge, SchemaUpdate, directed_edge, posting};
use ultragraphdb_store_posting::{
    BIT_DELTA_POSTING, CachedList, List, Lists, ListsConfig, PostingError,
};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, Stream};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

pub type TestLists = Lists<SimStore, SimListCache<CachedList>>;

/// A fault-free simulated store.
pub fn new_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

/// Lists over a fault-free simulated store with a cache of 1,024 entries.
pub fn new_lists() -> TestLists {
    lists_over(new_store(), ListsConfig::default())
}

/// Lists over `store` with a fresh cache.
pub fn lists_over(store: SimStore, config: ListsConfig) -> TestLists {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    Lists::new(store, cache, config)
}

/// The data key of `attr` in the root namespace for `uid`.
pub fn key(attr: &str, uid: u64) -> Vec<u8> {
    data_key(&attr_in_root_namespace(attr), uid).expect("a data key")
}

/// A set of the reference `uid`.
pub fn set_uid(uid: u64) -> DirectedEdge {
    DirectedEdge {
        value_id: uid,
        op: directed_edge::Op::Set as i32,
        ..DirectedEdge::default()
    }
}

/// A delete of the reference `uid`.
pub fn del_uid(uid: u64) -> DirectedEdge {
    DirectedEdge {
        op: directed_edge::Op::Del as i32,
        ..set_uid(uid)
    }
}

/// `edge` with a facet keyed `key`.
pub fn with_facet(mut edge: DirectedEdge, key: &str) -> DirectedEdge {
    edge.facets = vec![api::Facet {
        key: key.to_string(),
        ..api::Facet::default()
    }];
    edge
}

/// A set of the untagged string `value`.
pub fn set_value(value: &str) -> DirectedEdge {
    DirectedEdge {
        value: value.as_bytes().to_vec(),
        value_type: posting::ValType::String as i32,
        op: directed_edge::Op::Set as i32,
        ..DirectedEdge::default()
    }
}

/// A delete of the untagged string `value`.
pub fn del_value(value: &str) -> DirectedEdge {
    DirectedEdge {
        op: directed_edge::Op::Del as i32,
        ..set_value(value)
    }
}

/// A set of `value` tagged `lang`.
pub fn set_lang(value: &str, lang: &str) -> DirectedEdge {
    DirectedEdge {
        lang: lang.to_string(),
        ..set_value(value)
    }
}

/// A delete of every posting: `* *`.
pub fn delete_all() -> DirectedEdge {
    DirectedEdge {
        value: b"_STAR_ALL".to_vec(),
        op: directed_edge::Op::Del as i32,
        ..DirectedEdge::default()
    }
}

/// The schema of a predicate of one uid, or of a uid list.
pub fn uid_schema(list: bool) -> SchemaUpdate {
    SchemaUpdate {
        value_type: posting::ValType::Uid as i32,
        list,
        ..SchemaUpdate::default()
    }
}

/// Commits `edges` to `key` as one request reading at the newest timestamp: the commit timestamp, the one the
/// counter gives next.
pub fn commit_edges(
    lists: &TestLists,
    key: &[u8],
    edges: &[DirectedEdge],
    schema: Option<&SchemaUpdate>,
) -> u64 {
    let mut batch = lists.batch();
    let mut request = batch.request(lists.latest_ts());
    let list = request.get(key).expect("read");
    edges
        .iter()
        .for_each(|e| list.add_mutation(e, schema).expect("mutation"));
    batch.push(request);
    lists.commit(batch).expect("commit")[0]
}

/// The uids `list` yields at `read_ts` after `after_uid`: Go's `listToArray`.
pub fn uids(list: &List, read_ts: u64, after_uid: u64) -> Vec<u64> {
    let mut out = Vec::new();
    list.iterate(read_ts, after_uid, |p| {
        out.push(p.uid);
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .expect("iterate");
    out
}

/// Go's `checkUids`: the uids, and for three or more the reads after 10, 80 and 82.
pub fn check_uids(list: &List, expected: &[u64], read_ts: u64) {
    assert_eq!(uids(list, read_ts, 0), expected);
    if expected.len() >= 3 {
        assert_eq!(uids(list, read_ts, 10), expected[1..]);
        assert_eq!(uids(list, read_ts, 80), vec![81]);
        assert!(uids(list, read_ts, 82).is_empty());
    }
}

/// Applies `edge` to `list` in a request reading at `start_ts`.
pub fn add(list: &mut List, start_ts: u64, edge: &DirectedEdge) {
    list.set_ts(start_ts);
    list.add_mutation(edge, None).expect("mutation");
}

/// Writes `entries` to `store` in one batch.
pub fn write_entries(store: &SimStore, entries: Vec<Entry>) {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store.apply(batch, PersistMode::Buffer).expect("write");
}

/// Writes `list`'s rollup at `read_ts`, split at `max_list_size`, and reads the list back at the maximum
/// timestamp: Go's `Rollup`, `writePostingListToDisk` and `readPostingListFromDisk`. Returns the rollup's version.
pub fn roll_up(store: &SimStore, list: &List, read_ts: u64, max_list_size: usize) -> (List, u64) {
    let (version, entries) = list
        .rollup_entries(read_ts, max_list_size)
        .expect("rollup")
        .expect("a rollup at or above the list");
    write_entries(store, entries);
    (read_back(store, list.key()), version)
}

/// The list of `key` read from `store` at the maximum timestamp.
pub fn read_back(store: &SimStore, key: &[u8]) -> List {
    let snapshot = store.snapshot(u64::MAX).expect("snapshot");
    List::read(key, &snapshot).expect("read").0
}

/// A step a [`Probe`] runs inside an operation of its store.
pub type ProbeHook = Box<dyn FnMut() + Send>;

/// A simulated store that runs a test's step inside its next write batch, before the batch is written, or inside
/// its next snapshot, before the snapshot is opened: what lets a single-threaded test act while a commit is being
/// written or between a rollup's read and its write.
#[derive(Clone)]
pub struct Probe {
    inner: SimStore,
    on_apply: Arc<Mutex<Option<ProbeHook>>>,
    on_snapshot: Arc<Mutex<Option<ProbeHook>>>,
    on_drop: Arc<Mutex<Option<ProbeHook>>>,
}

impl std::fmt::Debug for Probe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Probe").finish_non_exhaustive()
    }
}

impl Probe {
    pub fn new(inner: SimStore) -> Self {
        Self {
            inner,
            on_apply: Arc::default(),
            on_snapshot: Arc::default(),
            on_drop: Arc::default(),
        }
    }

    pub fn inner(&self) -> &SimStore {
        &self.inner
    }

    /// Runs `hook` inside the next write batch, before it is written.
    pub fn on_next_apply(&self, hook: ProbeHook) {
        *self.on_apply.lock().expect("hook") = Some(hook);
    }

    /// Runs `hook` inside the next snapshot, before it is opened.
    pub fn on_next_snapshot(&self, hook: ProbeHook) {
        *self.on_snapshot.lock().expect("hook") = Some(hook);
    }

    /// Runs `hook` inside the next drop, of prefixes or of everything, before it removes anything.
    pub fn on_next_drop(&self, hook: ProbeHook) {
        *self.on_drop.lock().expect("hook") = Some(hook);
    }

    fn run(slot: &Mutex<Option<ProbeHook>>) {
        let hook = slot.lock().expect("hook").take();
        if let Some(mut hook) = hook {
            hook();
        }
    }
}

impl Store for Probe {
    type Snapshot = <SimStore as Store>::Snapshot;

    fn apply(&self, batch: WriteBatch, mode: PersistMode) -> Result<(), StoreError> {
        Self::run(&self.on_apply);
        self.inner.apply(batch, mode)
    }

    fn snapshot(&self, read_ts: u64) -> Result<Self::Snapshot, StoreError> {
        Self::run(&self.on_snapshot);
        self.inner.snapshot(read_ts)
    }

    fn drop_prefixes(
        &self,
        ts: u64,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
    ) -> Result<(), StoreError> {
        Self::run(&self.on_drop);
        self.inner.drop_prefixes(ts, prefixes, entries)
    }

    fn drop_all(&self, entries: Vec<Entry>) -> Result<(), StoreError> {
        Self::run(&self.on_drop);
        self.inner.drop_all(entries)
    }

    fn ban_namespace(&self, namespace: u64) -> Result<(), StoreError> {
        self.inner.ban_namespace(namespace)
    }

    fn set_discard_ts(&self, ts: u64) -> Result<(), StoreError> {
        self.inner.set_discard_ts(ts)
    }

    fn discard_ts(&self) -> u64 {
        self.inner.discard_ts()
    }

    fn set_tombstone_horizon(&self, ts: u64) -> Result<(), StoreError> {
        self.inner.set_tombstone_horizon(ts)
    }

    fn tombstone_horizon(&self) -> u64 {
        self.inner.tombstone_horizon()
    }

    fn applied_ts(&self) -> u64 {
        self.inner.applied_ts()
    }

    fn compact(&self) -> Result<(), StoreError> {
        self.inner.compact()
    }

    fn sweep_deletions(&self) -> Result<(), StoreError> {
        self.inner.sweep_deletions()
    }

    fn sync(&self) -> Result<(), StoreError> {
        self.inner.sync()
    }
}

pub type ProbeLists = Lists<Probe, SimListCache<CachedList>>;

/// Lists over a probe of a fault-free simulated store, shared so that a probe's hook can reach them.
pub fn probe_lists() -> (Arc<ProbeLists>, Probe) {
    let probe = Probe::new(new_store());
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let lists = Arc::new(Lists::new(probe.clone(), cache, ListsConfig::default()));
    (lists, probe)
}

/// Writes a delta of `edges` at `commit_ts` for `key` straight to `store`, recording `commit_ts` as applied, as
/// Go's tests commit by hand.
pub fn write_delta(store: &SimStore, key: &[u8], commit_ts: u64, edges: &[DirectedEdge]) {
    let mut list = List::empty(key.to_vec());
    list.set_ts(commit_ts.saturating_sub(1));
    edges
        .iter()
        .for_each(|e| list.add_mutation(e, None).expect("mutation"));
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(
        key.to_vec(),
        commit_ts,
        BIT_DELTA_POSTING,
        list.marshal_delta(),
    ));
    batch.set_applied_ts(commit_ts);
    store.apply(batch, PersistMode::Buffer).expect("write");
}

/// Moves `lists`' counter so that its next commit is at `commit_ts`.
pub fn next_commit_at<S: Store>(lists: &Lists<S, SimListCache<CachedList>>, commit_ts: u64) {
    lists.gate(commit_ts - 1).expect("gate");
    assert_eq!(
        lists.peek_ts(),
        commit_ts,
        "the counter is already past {commit_ts}"
    );
}
