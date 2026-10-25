/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A simulated store that runs a test's step right after its next snapshot is opened: what lets a single-threaded
//! test act between a rollup's read, which reads the state the snapshot holds, and its write.

use std::sync::{Arc, Mutex};

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_sim::SimStore;
use ultragraphdb_traits_store::{Entry, PersistMode, RebuildRecord, Store, Temporary, WriteBatch};

/// A step a [`Probe`] runs inside an operation of its store.
pub type ProbeHook = Box<dyn FnMut() + Send>;

#[derive(Clone)]
pub struct Probe {
    inner: SimStore,
    on_snapshot: Arc<Mutex<Option<ProbeHook>>>,
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
            on_snapshot: Arc::default(),
        }
    }

    pub fn inner(&self) -> &SimStore {
        &self.inner
    }

    /// Runs `hook` right after the next snapshot is opened.
    pub fn on_next_snapshot(&self, hook: ProbeHook) {
        *self.on_snapshot.lock().expect("hook") = Some(hook);
    }
}

impl Store for Probe {
    type Snapshot = <SimStore as Store>::Snapshot;

    fn apply(&self, batch: WriteBatch, mode: PersistMode) -> Result<(), StoreError> {
        self.inner.apply(batch, mode)
    }

    fn snapshot(&self, read_ts: u64) -> Result<Self::Snapshot, StoreError> {
        let snapshot = self.inner.snapshot(read_ts);
        let hook = self.on_snapshot.lock().expect("hook").take();
        if let Some(mut hook) = hook {
            hook();
        }
        snapshot
    }

    fn drop_prefixes(
        &self,
        ts: u64,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
    ) -> Result<(), StoreError> {
        self.inner.drop_prefixes(ts, prefixes, entries)
    }

    fn drop_all(&self, entries: Vec<Entry>) -> Result<(), StoreError> {
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

    fn create_temporary(&self) -> Result<Temporary, StoreError> {
        self.inner.create_temporary()
    }

    fn apply_temporary(&self, temporary: &Temporary, batch: WriteBatch) -> Result<(), StoreError> {
        self.inner.apply_temporary(temporary, batch)
    }

    fn temporary_snapshot(&self, temporary: &Temporary) -> Result<Self::Snapshot, StoreError> {
        self.inner.temporary_snapshot(temporary)
    }

    fn drop_temporary(&self, temporary: Temporary) -> Result<(), StoreError> {
        self.inner.drop_temporary(temporary)
    }

    fn write_rebuild_record(&self, record: &RebuildRecord) -> Result<(), StoreError> {
        self.inner.write_rebuild_record(record)
    }

    fn rebuild_records(&self) -> Result<Vec<RebuildRecord>, StoreError> {
        self.inner.rebuild_records()
    }
}
