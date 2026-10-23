/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use fjall::{OwnedWriteBatch, Readable};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::PersistMode;

use crate::types::fjall_store::Shared;
use crate::utils::lock::lock;
use crate::utils::stored_key::escaped_part;
use crate::utils::stored_value::decode_value;

impl Shared {
    /// Flushes the data keyspace's memtables and compacts every table into the last level, where the compaction
    /// filter collects what the discard timestamp allows; then syncs. lsm-tree runs the filter only when a
    /// compaction merges tables, never on a flush, so nothing else reaches data no later merge touches.
    pub fn compact(&self) -> Result<(), StoreError> {
        self.refuse_if_poisoned()?;
        self.data
            .rotate_memtable_and_wait()
            .map_err(|e| self.failed(e))?;
        self.data.major_compact().map_err(|e| self.failed(e))?;
        let _writer = lock(&self.writer);
        let mut batch = self.batch(PersistMode::Buffer);
        let collected = self.remove_collected(&mut batch);
        self.commit(batch)?;
        self.sync()?;
        tracing::info!(
            collected,
            discard_ts = self.gc.discard_ts(),
            "store compacted"
        );
        Ok(())
    }

    /// Adds a removal of every stored key compactions collected since the last call, so that fjall's replay of its
    /// journal at open does not bring them back. A collected version lies below a discard-flagged version or a
    /// deletion at or below the discard timestamp, which only rises, so no read the store serves returns it, and a
    /// later write of the same stored key is as invisible: the removal loses nothing a read can see.
    pub fn remove_collected(&self, batch: &mut OwnedWriteBatch) -> usize {
        let collected = self.gc.take_collected();
        let count = collected.len();
        collected
            .into_iter()
            .for_each(|key| batch.remove(&self.data, key));
        count
    }

    /// At one snapshot, removes every deletion at or below the tombstone horizon that has no version of its key
    /// below it, then syncs. The writer lock is held.
    pub fn sweep(&self) -> Result<(), StoreError> {
        self.refuse_if_poisoned()?;
        let horizon = lock(&self.view).horizon_in_effect();
        let snapshot = self.db.snapshot();
        let mut batch = self.synced_batch();
        // The collected versions' removals precede the deletions', in the journal as in the batch: a deletion the
        // sweep removes must not be replayed without what was below it.
        let collected = self.remove_collected(&mut batch);
        let mut swept = 0usize;
        // The key being read, its oldest version so far, and whether that version may be swept.
        let mut oldest: Option<(Vec<u8>, fjall::Slice, bool)> = None;
        for guard in snapshot.iter(&self.data) {
            let (stored, bytes) = guard.into_inner().map_err(|e| self.failed(e))?;
            let escaped = escaped_part(&stored).map(<[u8]>::to_vec);
            let sweepable = escaped.is_some()
                && decode_value(&bytes).is_ok_and(|v| v.deletion)
                && version_of(&stored) <= horizon;
            let same_key = matches!((&oldest, &escaped), (Some((k, _, _)), Some(e)) if k == e);
            if !same_key && let Some((_, key, true)) = oldest.take() {
                batch.remove(&self.data, key);
                swept += 1;
            }
            oldest = escaped.map(|e| (e, stored, sweepable));
        }
        if let Some((_, key, true)) = oldest {
            batch.remove(&self.data, key);
            swept += 1;
        }
        self.commit(batch)?;
        self.sync()?;
        tracing::info!(swept, collected, horizon, "deletions swept");
        Ok(())
    }
}

/// The version a stored key ends in; the caller has checked that it ends in one.
fn version_of(stored: &[u8]) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&stored[stored.len() - 8..]);
    !u64::from_be_bytes(raw)
}
