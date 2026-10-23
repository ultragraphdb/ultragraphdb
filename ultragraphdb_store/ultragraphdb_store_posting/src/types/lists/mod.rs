/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod lists_commit;
mod lists_drop;
mod lists_read;
mod lists_rollup;
pub(crate) mod state;

use std::sync::{Condvar, Mutex};

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::posting_error::PostingError;
use crate::types::cached_list::CachedList;
use crate::types::lists::state::State;
use crate::types::lists_config::ListsConfig;
use crate::types::rollup_queues::RollupQueues;
use crate::utils::lock::{lock, wait};

/// The posting lists of one store: the reads through the cache, the single writer of commits and rollups, the read
/// gate, the local timestamp counter and the rollup queues. What Dgraph spreads over `MemoryLayer`, the oracle,
/// `TxnWriter` and `incrRollupi`, without the pending-transaction layer: a request's writes reach the store only
/// in its commit batch.
///
/// **The writer.** One writer lock serialises commit batches, rollup batches, drops, epoch changes and cache fills.
/// A commit takes its timestamps from the counter under it, writes one store batch, updates the cache, and only
/// then advances the applied timestamp.
///
/// **The read gate.** A read at `t` waits until every commit timestamp assigned at or below `t` is applied, and
/// moves the counter past `t`, so no later commit lands at or below it.
///
/// **The cache.** An entry serves a read when its epoch is current and its timestamp is at or below the read's
/// ([`CachedList::serves`]); commits replace or remove the entries of the lists they write, and a fill is refused
/// when a commit wrote its key after the fill's snapshot or the epoch moved.
#[derive(Debug)]
pub struct Lists<S, C> {
    store: S,
    cache: C,
    config: ListsConfig,
    state: Mutex<State>,
    changed: Condvar,
    writer: Mutex<()>,
    rollups: Mutex<RollupQueues>,
    /// Held by a run of the rollup worker, so that runs, like Dgraph's one rollup goroutine, never overlap.
    rolling: Mutex<()>,
}

impl<S, C> Lists<S, C> {
    /// The shared state, which the drops of requests and cache misses reach without the store's bounds.
    pub(crate) fn state(&self) -> &Mutex<State> {
        &self.state
    }
}

/// Whether a read may go ahead.
enum Gate {
    Open,
    Wait,
}

impl<S, C> Lists<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The lists of `store`, cached in `cache`, as the store holds them when it is opened.
    pub fn new(store: S, cache: C, config: ListsConfig) -> Self {
        let applied = store.applied_ts();
        Self {
            store,
            cache,
            config,
            state: Mutex::new(State::new(applied)),
            changed: Condvar::new(),
            writer: Mutex::new(()),
            rollups: Mutex::new(RollupQueues::default()),
            rolling: Mutex::new(()),
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn cache(&self) -> &C {
        &self.cache
    }

    pub fn config(&self) -> ListsConfig {
        self.config
    }

    /// The highest commit timestamp applied.
    pub fn applied_ts(&self) -> u64 {
        lock(&self.state).applied
    }

    /// The cache's epoch.
    pub fn epoch(&self) -> u64 {
        lock(&self.state).epoch
    }

    /// A timestamp from the local counter that no one has had.
    pub fn fresh_ts(&self) -> u64 {
        lock(&self.state).ts.fresh()
    }

    /// The newest timestamp the counter has given: the read timestamp of a request that reads everything assigned
    /// so far, as Zero's `MaxAssigned` serves a read-only query.
    pub fn latest_ts(&self) -> u64 {
        lock(&self.state).ts.peek().saturating_sub(1)
    }

    /// The timestamp the counter gives next: the first commit timestamp of the next batch.
    pub fn peek_ts(&self) -> u64 {
        lock(&self.state).ts.peek()
    }

    /// The timestamp of the oldest open request, which bounds the discard timestamp.
    pub fn oldest_open_read(&self) -> Option<u64> {
        lock(&self.state).ts.oldest_open_read()
    }

    /// Whether a read at `read_ts` may go ahead now, without waiting.
    ///
    /// # Errors
    ///
    /// [`StoreError::Poisoned`] when a commit at or below `read_ts` failed to write.
    pub fn try_gate(&self, read_ts: u64) -> Result<bool, PostingError> {
        let state = lock(&self.state);
        Ok(matches!(Self::gate_of(&state, read_ts)?, Gate::Open))
    }

    /// Waits until every commit timestamp assigned at or below `read_ts` is applied, and moves the counter past
    /// `read_ts`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Poisoned`] when a commit at or below `read_ts` failed to write.
    pub fn gate(&self, read_ts: u64) -> Result<(), PostingError> {
        let mut state = lock(&self.state);
        state.ts.observe(read_ts);
        while let Gate::Wait = Self::gate_of(&state, read_ts)? {
            state = wait(&self.changed, state);
        }
        Ok(())
    }

    fn gate_of(state: &State, read_ts: u64) -> Result<Gate, PostingError> {
        if state.unapplied_from.is_some_and(|first| first <= read_ts) {
            return Err(StoreError::Poisoned().into());
        }
        Ok(match state.in_flight {
            Some(first) if first <= read_ts => Gate::Wait,
            _ => Gate::Open,
        })
    }

    /// Makes every batch written so far survive a power loss: what the owner of `process` durability calls once a
    /// minute.
    pub fn sync(&self) -> Result<(), PostingError> {
        Ok(self.store.sync()?)
    }

    /// Records a request open at `read_ts`.
    pub(crate) fn open_read(&self, read_ts: u64) {
        lock(&self.state).ts.open_read(read_ts);
    }

    /// Marks a write failed, which stops every later write until the store is reopened, and wakes the reads
    /// waiting on it.
    fn fail_writes(&self, unapplied_from: Option<u64>) {
        let mut state = lock(&self.state);
        state.write_failed = true;
        state.in_flight = None;
        if unapplied_from.is_some() {
            state.unapplied_from = unapplied_from;
        }
        drop(state);
        self.changed.notify_all();
    }

    /// Refuses a write once one has failed.
    fn refuse_if_failed(&self) -> Result<(), PostingError> {
        if lock(&self.state).write_failed {
            Err(StoreError::Poisoned().into())
        } else {
            Ok(())
        }
    }
}
