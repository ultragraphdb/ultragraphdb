/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::Ordering;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{
    Entry, PersistMode, RebuildRecord, Store, Temporary, WriteBatch, key_namespace,
};

use crate::types::drop_record::DropRecord;
use crate::types::fjall_snapshot::FjallSnapshot;
use crate::types::fjall_store::FjallStore;
use crate::types::fjall_store::store_meta::{
    APPLIED_KEY, DISCARD_KEY, HORIZON_KEY, banned_key, rebuild_key,
};
use crate::utils::lock::lock;
use crate::utils::stored_key::stored_key;
use crate::utils::stored_value::encode_value;

/// The collected stored keys a commit batch removes, beside its own entries, once background compactions have
/// collected that many: it bounds the memory they hold between explicit compactions.
const COLLECTED_IN_A_BATCH: usize = 100_000;

impl Store for FjallStore {
    type Snapshot = FjallSnapshot;

    /// One fjall batch, whatever its size, with the applied timestamp in the metadata keyspace beside its entries, its
    /// versionless writes in the keyspace without versions and the removal of the rebuild records it names.
    fn apply(&self, batch: WriteBatch, mode: PersistMode) -> Result<(), StoreError> {
        let s = &self.shared;
        let _writer = lock(&s.writer);
        s.refuse_if_poisoned()?;
        batch.check_key_sizes()?;
        let banned = lock(&s.view).banned.clone();
        let applied = batch
            .applied_ts()
            .map(|ts| ts.max(s.applied_ts.load(Ordering::Acquire)));
        let mut fb = s.batch(mode);
        if s.gc.collected_len() >= COLLECTED_IN_A_BATCH {
            s.remove_collected(&mut fb);
        }
        for id in batch.removed_rebuild_records() {
            fb.remove(&s.meta, rebuild_key(*id));
        }
        let (entries, versionless) = batch.into_parts();
        for e in versionless {
            match e.value() {
                Some(value) => fb.insert(&s.terms, e.key(), value),
                None => fb.remove(&s.terms, e.key()),
            }
        }
        for e in entries {
            if key_namespace(e.key()).is_some_and(|ns| banned.contains(&ns)) {
                continue;
            }
            fb.insert(
                &s.data,
                stored_key(e.key(), e.version()),
                encode_value(&e, false),
            );
        }
        if let Some(ts) = applied {
            fb.insert(&s.meta, APPLIED_KEY, ts.to_be_bytes().to_vec());
        }
        s.commit(fb)?;
        if let Some(ts) = applied {
            s.applied_ts.store(ts, Ordering::Release);
        }
        Ok(())
    }

    /// One point get, through the keyspace's bloom filters.
    fn get_versionless(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        self.shared
            .terms
            .get(key)
            .map(|v| v.map(|v| v.to_vec()))
            .map_err(|e| StoreError::ReadFailed(key.to_vec(), e.to_string()))
    }

    fn snapshot(&self, read_ts: u64) -> Result<Self::Snapshot, StoreError> {
        let s = &self.shared;
        let mut view = lock(&s.view);
        if read_ts < view.discard_in_effect() {
            return Err(StoreError::TsTooOld());
        }
        *view.readers.entry(read_ts).or_default() += 1;
        s.publish_discard(&view);
        let banned = view.banned.clone();
        let snapshot = s.db.snapshot();
        drop(view);
        Ok(FjallSnapshot::new(s.clone(), snapshot, read_ts, banned))
    }

    fn create_temporary(&self) -> Result<Temporary, StoreError> {
        self.shared.create_temporary()
    }

    fn apply_temporary(&self, temporary: &Temporary, batch: WriteBatch) -> Result<(), StoreError> {
        self.shared.apply_temporary(temporary, batch)
    }

    fn temporary_snapshot(&self, temporary: &Temporary) -> Result<Self::Snapshot, StoreError> {
        self.shared.temporary_snapshot(temporary)
    }

    fn drop_temporary(&self, temporary: Temporary) -> Result<(), StoreError> {
        self.shared.drop_temporary(temporary)
    }

    fn write_rebuild_record(&self, record: &RebuildRecord) -> Result<(), StoreError> {
        self.shared.write_rebuild_record(record)
    }

    fn rebuild_records(&self) -> Result<Vec<RebuildRecord>, StoreError> {
        self.shared.rebuild_records()
    }

    fn drop_prefixes(
        &self,
        ts: u64,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
    ) -> Result<(), StoreError> {
        self.drop_prefixes_observed(ts, prefixes, entries, |_| {})
    }

    fn drop_all(&self, entries: Vec<Entry>) -> Result<(), StoreError> {
        let _writer = lock(&self.shared.writer);
        self.shared.run_drop(
            DropRecord {
                ts: 0,
                prefixes: None,
                entries,
            },
            &mut |_| {},
        )
    }

    fn ban_namespace(&self, namespace: u64) -> Result<(), StoreError> {
        let s = &self.shared;
        let _writer = lock(&s.writer);
        s.refuse_if_poisoned()?;
        let mut batch = s.synced_batch();
        batch.insert(&s.meta, banned_key(namespace), Vec::new());
        s.commit(batch)?;
        let mut view = lock(&s.view);
        let mut banned = (*view.banned).clone();
        banned.insert(namespace);
        view.banned = Arc::new(banned);
        tracing::info!(namespace, "namespace banned");
        Ok(())
    }

    fn set_discard_ts(&self, ts: u64) -> Result<(), StoreError> {
        let s = &self.shared;
        let _writer = lock(&s.writer);
        s.refuse_if_poisoned()?;
        let asked = lock(&s.view).discard_ts.max(ts);
        let mut batch = s.synced_batch();
        batch.insert(&s.meta, DISCARD_KEY, asked.to_be_bytes().to_vec());
        s.commit(batch)?;
        let mut view = lock(&s.view);
        view.discard_ts = asked;
        s.publish_discard(&view);
        Ok(())
    }

    fn discard_ts(&self) -> u64 {
        lock(&self.shared.view).discard_in_effect()
    }

    fn set_tombstone_horizon(&self, ts: u64) -> Result<(), StoreError> {
        let s = &self.shared;
        let _writer = lock(&s.writer);
        s.refuse_if_poisoned()?;
        let mut batch = s.synced_batch();
        batch.insert(&s.meta, HORIZON_KEY, ts.to_be_bytes().to_vec());
        s.commit(batch)?;
        lock(&s.view).horizon = Some(ts);
        Ok(())
    }

    fn tombstone_horizon(&self) -> u64 {
        lock(&self.shared.view).horizon_in_effect()
    }

    fn applied_ts(&self) -> u64 {
        self.shared.applied_ts.load(Ordering::Acquire)
    }

    fn compact(&self) -> Result<(), StoreError> {
        self.shared.compact()
    }

    fn sweep_deletions(&self) -> Result<(), StoreError> {
        let _writer = lock(&self.shared.writer);
        self.shared.sweep()
    }

    fn sync(&self) -> Result<(), StoreError> {
        let _writer = lock(&self.shared.writer);
        self.shared.refuse_if_poisoned()?;
        self.shared.sync()
    }
}
