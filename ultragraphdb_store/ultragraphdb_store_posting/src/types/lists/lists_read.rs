/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/mvcc.go (ReadData, readFromCache, readFromDisk, saveInCache, checkForRollup,
 * GetNoStore). Changed for ultragraphdb: translated to Rust; a read passes the read gate first; a cache entry
 * serves a read at or above its timestamp in its epoch, where Dgraph serves only reads at or below the list's
 * newest version (issue #9597); a fill is checked under the writer lock and refused after a commit to its key or
 * an epoch change, where ristretto refuses only a fill older than the entry it would replace; reads at the
 * request's timestamp go through one snapshot, a list read below its newest complete version being read again at
 * the read's timestamp, as Dgraph does.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::Arc;

use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::posting_error::PostingError;
use crate::types::cache_miss::CacheMiss;
use crate::types::cached_list::CachedList;
use crate::types::list::List;
use crate::types::lists::Lists;
use crate::utils::constants::HIGH_PRIORITY_DELTAS;
use crate::utils::lock::lock;

impl<S, C> Lists<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The list of `key` at `read_ts`, from the cache when an entry serves the read, from the store otherwise,
    /// filling the cache with what it read when no commit got in between: Dgraph's `ReadData`.
    pub fn read(&self, key: &[u8], read_ts: u64) -> Result<List, PostingError> {
        if let Some(list) = self.cached(key, read_ts)? {
            return Ok(list);
        }
        let miss = self.miss(key, read_ts)?;
        let list = miss.list().clone();
        self.fill(miss);
        Ok(list)
    }

    /// The list of `key` at `read_ts` from the cache, after the read gate; `None` when no entry serves the read.
    pub fn cached(&self, key: &[u8], read_ts: u64) -> Result<Option<List>, PostingError> {
        self.gate(read_ts)?;
        let epoch = lock(&self.state).epoch;
        let Some(entry) = self.cache.get(key) else {
            return Ok(None);
        };
        if !entry.serves(read_ts, epoch) {
            return Ok(None);
        }
        let mut list = (**entry.list()).clone();
        list.set_ts(read_ts);
        if list.mutation().len() > HIGH_PRIORITY_DELTAS {
            self.add_rollup_key(key.to_vec(), 0);
        }
        Ok(Some(list))
    }

    /// Reads the list of `key` from the store at `read_ts`, after the read gate, and prepares the cache fill: the
    /// list as of the applied timestamp, read through the same snapshot when it can be.
    pub fn miss(&self, key: &[u8], read_ts: u64) -> Result<CacheMiss<'_>, PostingError> {
        self.gate(read_ts)?;
        let (base, epoch) = {
            let mut state = lock(&self.state);
            let base = state.applied;
            *state.fills.entry(base).or_default() += 1;
            (base, state.epoch)
        };
        // The miss registers its fill before it reads, so that the drop below ends it on every path.
        let mut miss = CacheMiss {
            state: &self.state,
            key: key.to_vec(),
            list: List::empty(key.to_vec()),
            fill: List::empty(key.to_vec()),
            base,
            epoch,
        };
        let newest = self.read_store(key, base.max(read_ts))?;
        miss.list = if newest.min_ts() <= read_ts {
            let mut list = newest.clone();
            list.set_ts(read_ts);
            list
        } else {
            self.read_store(key, read_ts)?
        };
        miss.fill = newest.without_scratch();
        Ok(miss)
    }

    /// Fills the cache with what `miss` read, unless a commit wrote its key after the miss's base or the epoch
    /// moved; checked under the writer lock. A fill that finds the writer lock held gives up rather than wait for
    /// it, so a read never waits for a commit, and a read from inside a commit cannot deadlock. Returns whether the
    /// fill was made.
    pub fn fill(&self, miss: CacheMiss<'_>) -> bool {
        let _writer = match self.writer.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return false,
        };
        let current = {
            let state = lock(&self.state);
            state.epoch == miss.epoch
                && state
                    .touched
                    .get(&miss.key)
                    .is_none_or(|&ts| ts <= miss.base)
        };
        if current {
            self.cache.insert(
                miss.key.clone(),
                CachedList::new(Arc::new(miss.fill.clone()), miss.base, miss.epoch),
            );
        }
        current
    }

    /// The list of `key` at `read_ts` from the store, after the read gate, bypassing the cache.
    pub fn read_uncached(&self, key: &[u8], read_ts: u64) -> Result<List, PostingError> {
        self.gate(read_ts)?;
        self.read_store(key, read_ts)
    }

    /// The list of `key` read through a snapshot at `read_ts`, queueing its key for a rollup when it has deltas.
    fn read_store(&self, key: &[u8], read_ts: u64) -> Result<List, PostingError> {
        let snapshot = self.store.snapshot(read_ts)?;
        let (list, deltas) = List::read(key, &snapshot)?;
        if deltas > 0 {
            let priority = usize::from(deltas <= HIGH_PRIORITY_DELTAS);
            self.add_rollup_key(key.to_vec(), priority);
        }
        Ok(list)
    }
}
