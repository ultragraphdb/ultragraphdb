/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests and the scenarios share: lists over a fault-free simulated store, quad keys and postings, and
//! the reads and writes the ported Go tests make by hand.

use std::ops::ControlFlow;
use std::panic::Location;
use std::sync::{Arc, Mutex, PoisonError};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use ultragraphdb_core_codec::{Facet, Posting, PostingType, UidPack, encode};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::{
    BIT_DELTA_POSTING, CachedList, DEL, List, Lists, ListsConfig, PostingError, SET,
    delete_all_posting,
};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, Stream};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};
use xxhash_rust::xxh3::xxh3_64;

/// Lists over a simulated store and cache.
pub type TestLists = Lists<SimStore, SimListCache<CachedList>>;

/// The error of a result a test expects to fail, where it would otherwise `unwrap_err` it.
pub trait Failure<E> {
    /// The error, or a [`TestError`] saying `what` succeeded, naming the caller's line.
    fn failure(self, what: &str) -> TestResult<E>;
}

impl<T, E> Failure<E> for Result<T, E> {
    #[track_caller]
    fn failure(self, what: &str) -> TestResult<E> {
        let at = Location::caller();
        match self {
            Ok(_) => Err(TestError::new(format!("{what}: succeeded (at {at})"))),
            Err(e) => Ok(e),
        }
    }
}

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

/// The posting key of the reference the Go tests number `n`: the system ID of type 1, sequence `n` and bucket 0 as
/// its unsigned 128-bit value, written from the ID's bit layout (type in bits 127–112, sequence in bits 111–48). The
/// map keeps the Go tests' order, so a read after `n` is a read after `rid(n)`; `n` = 0 gives no valid ID.
pub const fn rid(n: u64) -> u128 {
    (1 << 112) | ((n as u128) << 48)
}

/// The posting keys of the references numbered `ns`, in their order.
pub fn rids(ns: &[u64]) -> Vec<u128> {
    ns.iter().map(|&n| rid(n)).collect()
}

/// The system ID of the reference numbered `n`, [`rid`] decoded.
pub fn id(n: u64) -> TestResult<SystemId> {
    SystemId::from_u128(rid(n)).context("a valid ID")
}

/// The system IDs of the references numbered `ns`, in their order.
pub fn ids(ns: &[u64]) -> TestResult<Vec<SystemId>> {
    ns.iter().map(|&n| id(n)).collect()
}

/// The pack of `ids`, which holds at least one, in blocks of `block_size`.
pub fn pack_of(ids: &[SystemId], block_size: usize) -> TestResult<UidPack> {
    encode(ids, block_size)
        .context("the IDs pack")?
        .context("a pack of at least one ID")
}

/// The number [`rid`] gave the reference key `key`: its sequence.
pub fn number(key: u128) -> TestResult<u64> {
    u64::try_from((key >> 48) & u128::from(u64::MAX)).context("a sequence of 64 bits")
}

/// The sequence the tests give the predicate `attr`: drawn from its name, never 0.
fn predicate_number(attr: &str) -> u64 {
    xxh3_64(attr.as_bytes()).max(1)
}

/// The ID the tests give the predicate `attr`.
pub fn predicate(attr: &str) -> TestResult<SystemId> {
    id(predicate_number(attr))
}

/// The forward key of `attr` for the subject numbered `n` (at least 1) in the default graph of the root tenant,
/// written from the key's layout: the forward keyspace's byte 0x10, the tenant's 8 big-endian bytes, the default
/// graph's 16 zero bytes, then the predicate's and the subject's IDs as 16 big-endian bytes each.
pub fn key(attr: &str, n: u64) -> Vec<u8> {
    let mut key = vec![0x10];
    key.extend_from_slice(&0u64.to_be_bytes());
    key.extend_from_slice(&[0; 16]);
    key.extend_from_slice(&rid(predicate_number(attr)).to_be_bytes());
    key.extend_from_slice(&rid(n.max(1)).to_be_bytes());
    key
}

/// A set of the reference numbered `n`.
pub fn set_uid(n: u64) -> Posting {
    Posting::default().with_uid(rid(n)).with_op(SET)
}

/// A delete of the reference numbered `n`.
pub fn del_uid(n: u64) -> Posting {
    set_uid(n).with_op(DEL)
}

/// `posting` with a facet keyed `key`.
pub fn with_facet(posting: Posting, key: &str) -> Posting {
    posting.with_facets(vec![Facet::new(key)])
}

/// A set of the value `value`, as a quad list holds a literal: its bytes, under their fingerprint.
pub fn set_value(value: &str) -> Posting {
    Posting::default()
        .with_uid(u128::from(xxh3_64(value.as_bytes())))
        .with_value(value.as_bytes().to_vec())
        .with_posting_type(PostingType::Value)
        .with_op(SET)
}

/// A delete of the value `value`.
pub fn del_value(value: &str) -> Posting {
    set_value(value).with_op(DEL)
}

/// A delete of every posting.
pub fn delete_all() -> Posting {
    delete_all_posting()
}

/// Commits `postings` to `key` as one request of a batch reading at the watermark: the commit version, the next even
/// one.
pub fn commit_edges(lists: &TestLists, key: &[u8], postings: &[Posting]) -> TestResult<u64> {
    let mut batch = lists.batch();
    let mut request = batch.request(lists.applied_ts());
    let list = request.get(key).context("read")?;
    postings
        .iter()
        .for_each(|p| list.add_posting(p.clone(), false));
    batch.push(request);
    lists
        .commit(batch)
        .context("commit")?
        .first()
        .copied()
        .context("the request's commit version")
}

/// Commits `postings` to `key` as [`commit_edges`] does, for a caller with no use for the commit version.
pub fn commit(lists: &TestLists, key: &[u8], postings: &[Posting]) -> TestResult {
    let _version: u64 = commit_edges(lists, key, postings)?;
    Ok(())
}

/// The posting keys `list` yields at `read_ts` after `after_uid`: Go's `listToArray`.
pub fn uids(list: &List, read_ts: u64, after_uid: u128) -> TestResult<Vec<u128>> {
    let mut out = Vec::new();
    list.iterate(read_ts, after_uid, |p| {
        out.push(p.uid());
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .context("iterate")?;
    Ok(out)
}

/// The values `list` holds at `read_ts`, in posting order.
pub fn values(list: &List, read_ts: u64) -> TestResult<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    list.iterate(read_ts, 0, |p| {
        out.push(p.value().to_vec());
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .context("iterate")?;
    Ok(out)
}

/// Go's `checkUids`: the references numbered `expected`, and for three or more the reads after 10, 80 and 82.
pub fn check_uids(list: &List, expected: &[u64], read_ts: u64) -> TestResult {
    let expected = rids(expected);
    assert_eq!(uids(list, read_ts, 0)?, expected);
    if expected.len() >= 3 {
        assert_eq!(uids(list, read_ts, rid(10))?, expected[1..]);
        assert_eq!(uids(list, read_ts, rid(80))?, vec![rid(81)]);
        assert!(uids(list, read_ts, rid(82))?.is_empty());
    }
    Ok(())
}

/// Applies `posting` to `list` in a request reading at `start_ts`.
pub fn add(list: &mut List, start_ts: u64, posting: &Posting) {
    list.set_ts(start_ts);
    list.add_posting(posting.clone(), false);
}

/// Writes `entries` to `store` in one batch.
pub fn write_entries(store: &SimStore, entries: Vec<Entry>) -> TestResult {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store.apply(batch, PersistMode::Buffer).context("write")
}

/// Writes `list`'s rollup at `read_ts`, split at `max_list_size`, and reads the list back at the maximum
/// timestamp: Go's `Rollup`, `writePostingListToDisk` and `readPostingListFromDisk`. Returns the rollup's version.
pub fn roll_up(
    store: &SimStore,
    list: &List,
    read_ts: u64,
    max_list_size: usize,
) -> TestResult<(List, u64)> {
    let (version, entries) = list
        .rollup_entries(read_ts, max_list_size)
        .context("rollup")?
        .context("a rollup at or above the list")?;
    write_entries(store, entries)?;
    Ok((read_back(store, list.key())?, version))
}

/// The list of `key` read from `store` at the maximum timestamp.
pub fn read_back(store: &SimStore, key: &[u8]) -> TestResult<List> {
    let snapshot = store.snapshot(u64::MAX).context("snapshot")?;
    Ok(List::read(key, &snapshot).context("read")?.0)
}

/// A step a [`Probe`] runs inside an operation of its store; its failure fails the operation.
pub type ProbeHook = Box<dyn FnMut() -> TestResult + Send>;

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
    /// A probe of `inner`, with no hook set.
    pub fn new(inner: SimStore) -> Self {
        Self {
            inner,
            on_apply: Arc::default(),
            on_snapshot: Arc::default(),
            on_drop: Arc::default(),
        }
    }

    /// The probed store.
    pub fn inner(&self) -> &SimStore {
        &self.inner
    }

    /// Runs `hook` inside the next write batch, before it is written.
    pub fn on_next_apply(&self, hook: ProbeHook) {
        *self.on_apply.lock().unwrap_or_else(PoisonError::into_inner) = Some(hook);
    }

    /// Runs `hook` inside the next snapshot, before it is opened.
    pub fn on_next_snapshot(&self, hook: ProbeHook) {
        *self
            .on_snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(hook);
    }

    /// Runs `hook` inside the next drop, of prefixes or of everything, before it removes anything.
    pub fn on_next_drop(&self, hook: ProbeHook) {
        *self.on_drop.lock().unwrap_or_else(PoisonError::into_inner) = Some(hook);
    }

    /// Runs the hook in `slot`, if any, once: a failed hook is a failed write, which fails the operation running it.
    fn run(slot: &Mutex<Option<ProbeHook>>) -> Result<(), StoreError> {
        let hook = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
        match hook {
            Some(mut hook) => hook()
                .map_err(|e| StoreError::write_failed(format!("the probe's hook failed: {e}"))),
            None => Ok(()),
        }
    }
}

impl Store for Probe {
    type Snapshot = <SimStore as Store>::Snapshot;

    fn apply(&self, batch: WriteBatch, mode: PersistMode) -> Result<(), StoreError> {
        Self::run(&self.on_apply)?;
        self.inner.apply(batch, mode)
    }

    fn get_versionless(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.get_versionless(key)
    }

    fn snapshot(&self, read_ts: u64) -> Result<Self::Snapshot, StoreError> {
        Self::run(&self.on_snapshot)?;
        self.inner.snapshot(read_ts)
    }

    fn drop_prefixes(
        &self,
        ts: u64,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
    ) -> Result<(), StoreError> {
        Self::run(&self.on_drop)?;
        self.inner.drop_prefixes(ts, prefixes, entries)
    }

    fn drop_all(&self, entries: Vec<Entry>) -> Result<(), StoreError> {
        Self::run(&self.on_drop)?;
        self.inner.drop_all(entries)
    }

    fn ban_tenant(&self, tenant: u64) -> Result<(), StoreError> {
        self.inner.ban_tenant(tenant)
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

/// Lists over a [`Probe`], with a simulated cache.
pub type ProbeLists = Lists<Probe, SimListCache<CachedList>>;

/// Lists over a probe of a fault-free simulated store, shared so that a probe's hook can reach them.
pub fn probe_lists() -> (Arc<ProbeLists>, Probe) {
    let probe = Probe::new(new_store());
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let lists = Arc::new(Lists::new(probe.clone(), cache, ListsConfig::default()));
    (lists, probe)
}

/// Writes a delta of `postings` at `commit_ts` for `key` straight to `store`, recording `commit_ts` as applied, as
/// Go's tests commit by hand.
pub fn write_delta(
    store: &SimStore,
    key: &[u8],
    commit_ts: u64,
    postings: &[Posting],
) -> TestResult {
    let mut list = List::empty(key.to_vec());
    list.set_ts(commit_ts.saturating_sub(1));
    postings
        .iter()
        .for_each(|p| list.add_posting(p.clone(), false));
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(
        key.to_vec(),
        commit_ts,
        BIT_DELTA_POSTING,
        list.marshal_delta(),
    ));
    batch.set_applied_ts(commit_ts);
    store.apply(batch, PersistMode::Buffer).context("write")
}

/// Commits `postings` to `key` as one transaction reading the list at the watermark, in a round of its own at
/// `version`: the version.
pub fn commit_edges_at<S: Store>(
    lists: &Lists<S, SimListCache<CachedList>>,
    key: &[u8],
    postings: &[Posting],
    version: u64,
) -> TestResult<u64> {
    let mut request = lists.request(lists.applied_ts());
    let list = request.get(key).context("read")?;
    postings
        .iter()
        .for_each(|p| list.add_posting(p.clone(), false));
    let writes = request.into_writes();
    lists
        .commit_round(&[(version, &writes)])
        .context("the round is written")?;
    Ok(version)
}

/// Commits `postings` to `key` at `version` as [`commit_edges_at`] does, for a caller with no use for the version.
pub fn commit_at<S: Store>(
    lists: &Lists<S, SimListCache<CachedList>>,
    key: &[u8],
    postings: &[Posting],
    version: u64,
) -> TestResult {
    let _version: u64 = commit_edges_at(lists, key, postings, version)?;
    Ok(())
}
