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

/// The posting lists of one store: the reads through the cache, the single writer of rounds, batches and rollups, the
/// read gate, the visibility watermark and the rollup queues. What Dgraph spreads over `MemoryLayer`, the oracle,
/// `TxnWriter` and `incrRollupi`, without the pending-transaction layer: a transaction's writes reach the store only
/// in its round's batch.
///
/// **The writer.** One writer lock serialises rounds, batches, rollup batches, drops, epoch changes and cache fills.
/// A round is written at the commit versions the commit step gave its transactions as one store batch, the cache is
/// updated, and only then does the watermark pass it. A batch and a drop, which the commit step does not order, take
/// the next even versions above the watermark.
///
/// **The read gate.** A read at the watermark or below never waits. A read above it waits until the watermark reaches
/// it, and is refused once a write has failed, as nothing then moves the watermark; a read at `u64::MAX` reads
/// everything applied, waiting only for a write under way.
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

    /// The visibility watermark: the highest commit version applied, written with its cache updates made. A
    /// transaction reads at it.
    pub fn applied_ts(&self) -> u64 {
        lock(&self.state).applied
    }

    /// The cache's epoch.
    pub fn epoch(&self) -> u64 {
        lock(&self.state).epoch
    }

    /// The timestamp of the oldest open request, which bounds the discard timestamp.
    pub fn oldest_open_read(&self) -> Option<u64> {
        lock(&self.state).ts.oldest_open_read()
    }

    /// Whether a read at `read_ts` may go ahead now, without waiting.
    ///
    /// # Errors
    ///
    /// As [`Lists::gate`].
    pub fn try_gate(&self, read_ts: u64) -> Result<bool, PostingError> {
        let state = lock(&self.state);
        Ok(matches!(Self::gate_of(&state, read_ts)?, Gate::Open))
    }

    /// Waits until the watermark reaches `read_ts`, or, at `u64::MAX`, until no write is under way.
    ///
    /// # Errors
    ///
    /// [`StoreError::Poisoned`] when a write at or below `read_ts` failed, or when one failed and `read_ts` is above
    /// the watermark, which then never reaches it.
    pub fn gate(&self, read_ts: u64) -> Result<(), PostingError> {
        let mut state = lock(&self.state);
        while let Gate::Wait = Self::gate_of(&state, read_ts)? {
            state = wait(&self.changed, state);
        }
        Ok(())
    }

    fn gate_of(state: &State, read_ts: u64) -> Result<Gate, PostingError> {
        if state.unapplied_from.is_some_and(|first| first <= read_ts) {
            return Err(StoreError::Poisoned().into());
        }
        let waits = if read_ts == u64::MAX {
            state.in_flight.is_some()
        } else {
            state.applied < read_ts
        };
        if waits && state.write_failed {
            return Err(StoreError::Poisoned().into());
        }
        Ok(if waits { Gate::Wait } else { Gate::Open })
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
