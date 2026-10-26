/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::Ordering;

use fjall::Readable;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{PersistMode, WriteBatch, key_namespace};

use crate::types::drop_record::DropRecord;
use crate::types::drop_stage::DropStage;
use crate::types::fjall_store::Shared;
use crate::types::fjall_store::store_meta::{
    APPLIED_KEY, BANNED_PREFIX, DISCARD_KEY, DROP_KEY, HORIZON_KEY, REBUILD_PREFIX,
};
use crate::utils::lock::lock;
use crate::utils::stored_key::{escape, stored_key};
use crate::utils::stored_value::encode_value;

/// The removals one fjall batch of a drop holds; a drop removes its data in as many batches as it needs.
const REMOVALS_PER_BATCH: usize = 10_000;

impl Shared {
    /// A drop through its record, the writer lock held: the record written and synced, then the drop finished, each
    /// stage reported to `observe` as it is reached.
    pub fn run_drop(
        &self,
        record: DropRecord,
        observe: &mut impl FnMut(DropStage),
    ) -> Result<(), StoreError> {
        self.refuse_if_poisoned()?;
        let mut check = WriteBatch::new();
        record.entries.iter().cloned().for_each(|e| check.push(e));
        check.check_key_sizes()?;
        let mut batch = self.synced_batch();
        batch.insert(&self.meta, DROP_KEY, record.encode());
        self.commit(batch)?;
        observe(DropStage::RecordSynced);
        self.finish_drop(&record, observe)?;
        tracing::info!(
            ts = record.ts,
            prefixes = record.prefixes.as_ref().map(Vec::len),
            entries = record.entries.len(),
            "dropped"
        );
        Ok(())
    }

    /// Removes what `record` covers, writes its entries and syncs, then deletes the record: the drop's work after
    /// its record is synced, done again in whole when a store opens to find the record. Each stage reached is
    /// reported to `observe`.
    pub fn finish_drop(
        &self,
        record: &DropRecord,
        observe: &mut impl FnMut(DropStage),
    ) -> Result<(), StoreError> {
        let fenced = match &record.prefixes {
            Some(prefixes) => {
                self.remove_under(prefixes, observe)?;
                true
            }
            None => {
                self.data.clear().map_err(|e| self.failed(e))?;
                self.terms.clear().map_err(|e| self.failed(e))?;
                self.reset_meta()?;
                // A clear leaves no removed version a compaction could still read.
                false
            }
        };
        observe(DropStage::DataRemoved);
        let banned = lock(&self.view).banned.clone();
        let mut batch = self.batch(PersistMode::Buffer);
        record
            .entries
            .iter()
            .filter(|e| key_namespace(e.key()).is_none_or(|ns| !banned.contains(&ns)))
            .for_each(|e| {
                batch.insert(
                    &self.data,
                    stored_key(e.key(), e.version()),
                    encode_value(e, fenced),
                )
            });
        self.commit(batch)?;
        self.sync()?;
        observe(DropStage::EntriesSynced);
        let mut batch = self.batch(PersistMode::Buffer);
        batch.remove(&self.meta, DROP_KEY);
        self.commit(batch)
    }

    /// Removes every stored version under `prefixes`, read at one snapshot, with lsm-tree removals, reporting the
    /// count removed after each full batch.
    fn remove_under(
        &self,
        prefixes: &[Vec<u8>],
        observe: &mut impl FnMut(DropStage),
    ) -> Result<(), StoreError> {
        let mut removed = 0usize;
        let snapshot = self.db.snapshot();
        let mut batch = self.batch(PersistMode::Buffer);
        let mut held = 0;
        for prefix in prefixes {
            for guard in snapshot.prefix(&self.data, escape(prefix)) {
                let key = guard.key().map_err(|e| self.failed(e))?;
                batch.remove(&self.data, key);
                held += 1;
                removed += 1;
                if held == REMOVALS_PER_BATCH {
                    self.commit(std::mem::replace(
                        &mut batch,
                        self.batch(PersistMode::Buffer),
                    ))?;
                    held = 0;
                    observe(DropStage::Removing(removed));
                }
            }
        }
        self.commit(batch)?;
        tracing::debug!(
            removed,
            prefixes = prefixes.len(),
            "removed the versions under dropped prefixes"
        );
        Ok(())
    }

    /// A drop of everything clears the applied timestamp, the banned namespaces, the discard timestamp, the
    /// tombstone horizon and the rebuild records.
    fn reset_meta(&self) -> Result<(), StoreError> {
        let mut batch = self.batch(PersistMode::Buffer);
        [APPLIED_KEY, DISCARD_KEY, HORIZON_KEY]
            .into_iter()
            .for_each(|k| batch.remove(&self.meta, k));
        let snapshot = self.db.snapshot();
        for prefix in [BANNED_PREFIX, REBUILD_PREFIX] {
            for guard in snapshot.prefix(&self.meta, prefix) {
                batch.remove(&self.meta, guard.key().map_err(|e| self.failed(e))?);
            }
        }
        self.commit(batch)?;
        self.applied_ts.store(0, Ordering::Release);
        let mut view = lock(&self.view);
        view.banned = Arc::default();
        view.discard_ts = 0;
        view.horizon = None;
        self.publish_discard(&view);
        Ok(())
    }
}
