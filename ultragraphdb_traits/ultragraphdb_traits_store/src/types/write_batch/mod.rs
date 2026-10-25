/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;

use crate::types::entry::Entry;
use crate::utils::key_size::check_key_size;

/// The writes [`crate::Store::apply`] makes atomically: entries in order, each naming its key and version, so one
/// batch may hold several commits at their own timestamps, the applied timestamp the batch records, and the rebuild
/// records it removes. Of two entries for the same key and version, the later is the one written.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct WriteBatch {
    entries: Vec<Entry>,
    applied_ts: Option<u64>,
    removed_rebuild_records: Vec<u64>,
}

impl WriteBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
    }

    /// Records `ts` as applied in this batch: the store's applied timestamp becomes the larger of it and `ts`. A
    /// commit batch records its highest commit timestamp; rollup batches record none.
    pub fn set_applied_ts(&mut self, ts: u64) {
        self.applied_ts = Some(ts);
    }

    /// Removes the rebuild record `id` ([`crate::RebuildRecord`]) in this batch, atomically with its entries: what a
    /// rebuild's swap batch and its failure batch do. A record the store does not hold is left alone.
    pub fn remove_rebuild_record(&mut self, id: u64) {
        self.removed_rebuild_records.push(id);
    }

    /// The rebuild records the batch removes, in the order named.
    pub fn removed_rebuild_records(&self) -> &[u64] {
        &self.removed_rebuild_records
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn into_entries(self) -> Vec<Entry> {
        self.entries
    }

    pub fn applied_ts(&self) -> Option<u64> {
        self.applied_ts
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The refusal of the first entry whose key is over a size limit, as [`crate::check_key_size`] gives it.
    pub fn check_key_sizes(&self) -> Result<(), StoreError> {
        self.entries
            .iter()
            .try_for_each(|e| check_key_size(e.key()))
    }
}
