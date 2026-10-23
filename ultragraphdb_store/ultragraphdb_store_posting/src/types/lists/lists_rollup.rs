/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/mvcc.go (incrRollupi.rollUpKey, Process, doRollup, addKeyToBatch). Changed
 * for ultragraphdb: translated to Rust; the worker runs on the caller's executor, each run taking the batches that
 * wait; a rollup writes its main key, its parts and the deletions of the parts it removes as one batch at the main
 * key's version through the writer, where Dgraph's TxnWriter commits each key on its own; a rollup is discarded
 * when the cache's epoch moved after its read (a drop, a schema change or a rebuild) or its key gained a deletion
 * above what it read; a list without deltas is not rewritten; rollups pause while a rebuild runs, and a paused
 * rollup returns `HighPriorityOp` as Dgraph's cancelled one does.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::Arc;

use ultragraphdb_error_store::StoreErrorEnum;
use ultragraphdb_traits_store::{
    Executor, ListCache, PersistMode, Store, StoreSnapshot, WriteBatch,
};

use crate::errors::posting_error::{PostingError, PostingErrorEnum};
use crate::types::cached_list::CachedList;
use crate::types::list::List;
use crate::types::lists::Lists;
use crate::utils::lock::{lock, wait};

/// How often the worker reports the number of keys it rolled up, as Dgraph does every thousand.
const REPORT_EVERY: u64 = 1000;

impl<S, C> Lists<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Queues `key` for a rollup at `priority`, 0 high and 1 low: Dgraph's `addKeyToBatch`.
    pub fn add_rollup_key(&self, key: Vec<u8>, priority: usize) {
        lock(&self.rollups).add_key(key, priority);
    }

    /// How many keys wait for a rollup.
    pub fn rollups_waiting(&self) -> usize {
        lock(&self.rollups).waiting()
    }

    /// Pauses rollups, as an index rebuild does: a rollup under way returns before it writes, and no new one
    /// starts until every pause is resumed.
    pub fn pause_rollups(&self) {
        lock(&self.state).paused += 1;
        self.changed.notify_all();
    }

    /// Ends one [`Lists::pause_rollups`].
    pub fn resume_rollups(&self) {
        let mut state = lock(&self.state);
        state.paused = state.paused.saturating_sub(1);
    }

    /// Rolls up every batch of keys waiting, deduplicating the keys rolled up in the last ten seconds of `now_secs`:
    /// one turn of Dgraph's `Process` loop. Returns how many keys it rolled up. A key that cannot be rolled up is
    /// reported and skipped, as Dgraph does.
    ///
    /// # Errors
    ///
    /// The store's error when a rollup batch cannot be written, after which writes stop until the store is
    /// reopened.
    pub fn run_rollups(&self, now_secs: u64) -> Result<usize, PostingError> {
        let _running = lock(&self.rolling);
        let mut rolled = 0usize;
        loop {
            let Some(batch) = lock(&self.rollups).take_batch() else {
                return Ok(rolled);
            };
            for key in batch {
                if !lock(&self.rollups).due(&key, now_secs) {
                    continue;
                }
                match self.rollup_key(&key) {
                    Ok(true) => {
                        rolled += 1;
                        if (rolled as u64).is_multiple_of(REPORT_EVERY) {
                            tracing::debug!(rolled, "rolled up keys");
                        }
                    }
                    Ok(false) => {}
                    Err(PostingError(PostingErrorEnum::Store(error)))
                        if matches!(
                            error.0,
                            StoreErrorEnum::Poisoned | StoreErrorEnum::WriteFailed(_)
                        ) =>
                    {
                        return Err(error.into());
                    }
                    Err(error) => {
                        tracing::warn!(key = %hex(&key), %error, "error rolling up key");
                    }
                }
            }
        }
    }

    /// Starts [`Lists::run_rollups`] on `executor`.
    pub fn spawn_rollups<E: Executor>(self: &Arc<Self>, executor: &E, now_secs: u64)
    where
        S: Send + Sync + 'static,
        C: Send + Sync + 'static,
    {
        let lists = Arc::clone(self);
        executor.spawn(move || {
            if let Err(error) = lists.run_rollups(now_secs) {
                tracing::error!(%error, "rollups stopped; the store must be reopened");
            }
        });
    }

    /// Rolls up the list of `key`: takes a fresh timestamp, waits until every commit below it is applied, reads the
    /// list there and writes its rollup through the writer, removing the key's cache entry: Dgraph's `rollUpKey`.
    /// Returns whether it wrote; it writes nothing for a list without deltas, and discards a rollup whose epoch
    /// moved or whose key gained a deletion above what it read.
    ///
    /// # Errors
    ///
    /// [`PostingError::HighPriorityOp`] while rollups are paused, the store's errors, and the list's.
    pub fn rollup_key(&self, key: &[u8]) -> Result<bool, PostingError> {
        let (read_ts, epoch) = self.rollup_start()?;
        let list = {
            let snapshot = self.store.snapshot(read_ts)?;
            List::read(key, &snapshot)?.0
        };
        if list.mutation().is_empty() {
            return Ok(false);
        }
        let Some((version, entries)) = list.rollup_entries(read_ts, self.config.max_list_size())?
        else {
            return Ok(false);
        };

        let _writer = lock(&self.writer);
        self.refuse_if_failed()?;
        {
            let state = lock(&self.state);
            if state.paused > 0 {
                return Err(PostingError::HighPriorityOp());
            }
            if state.epoch != epoch {
                tracing::info!(key = %hex(key), rollup_ts = read_ts, "rollup discarded: a drop or schema change came after its read");
                return Ok(false);
            }
        }
        if !self.still_holds(key, list.max_ts())? {
            tracing::info!(key = %hex(key), rollup_ts = read_ts, "rollup discarded: its key gained a deletion after its read");
            return Ok(false);
        }
        let mut batch = WriteBatch::new();
        entries.into_iter().for_each(|e| batch.push(e));
        if let Err(error) = self.store.apply(batch, PersistMode::Buffer) {
            tracing::error!(key = %hex(key), version, %error, "rollup batch failed; the store must be reopened");
            self.fail_writes(None);
            return Err(error.into());
        }
        self.cache.remove(key);
        Ok(true)
    }

    /// A fresh timestamp for a rollup, once every commit below it is applied, and the cache's epoch then.
    fn rollup_start(&self) -> Result<(u64, u64), PostingError> {
        let mut state = lock(&self.state);
        if state.paused > 0 {
            return Err(PostingError::HighPriorityOp());
        }
        let ts = state.ts.fresh();
        while state.in_flight.is_some_and(|first| first < ts) {
            state = wait(&self.changed, state);
            if state.paused > 0 {
                return Err(PostingError::HighPriorityOp());
            }
        }
        if state.unapplied_from.is_some_and(|first| first < ts) {
            return Err(ultragraphdb_error_store::StoreError::Poisoned().into());
        }
        Ok((ts, state.epoch))
    }

    /// Whether the newest versions of `key` still reach down to `max_ts`, the newest version the rollup read: a
    /// deletion written above it ends them before.
    fn still_holds(&self, key: &[u8], max_ts: u64) -> Result<bool, PostingError> {
        let snapshot = self.store.snapshot(u64::MAX)?;
        for item in snapshot.versions(key) {
            let item = item?;
            if item.version() <= max_ts {
                return Ok(item.version() == max_ts);
            }
        }
        Ok(false)
    }
}

/// A key as Go's `%x` prints it.
fn hex(key: &[u8]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}
