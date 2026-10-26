/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::FjallSnapshot;
use ultragraphdb_traits_store::{Entry, PersistMode, RebuildRecord, Store, Temporary, WriteBatch};

use crate::types::timed_store::TimedStore;
use crate::types::write_call::WriteCall;

impl Store for TimedStore {
    type Snapshot = FjallSnapshot;

    #[allow(
        clippy::disallowed_methods,
        reason = "a benchmark times each write call by the wall clock; the store under it reads no clock"
    )]
    fn apply(&self, batch: WriteBatch, mode: PersistMode) -> Result<(), StoreError> {
        let (entries, commit) = (batch.len(), batch.applied_ts().is_some());
        let start = Instant::now();
        let result = self.inner.apply(batch, mode);
        let nanos = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.record(WriteCall::new(
            nanos,
            entries,
            commit,
            self.inner.l0_table_count(),
            self.inner.sealed_memtable_count(),
        ));
        result
    }

    fn get_versionless(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.get_versionless(key)
    }

    fn snapshot(&self, read_ts: u64) -> Result<Self::Snapshot, StoreError> {
        self.inner.snapshot(read_ts)
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
