/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{Entry, ListCache, Store};

use crate::errors::posting_error::PostingError;
use crate::types::cached_list::CachedList;
use crate::types::lists::Lists;
use crate::utils::lock::lock;

impl<S, C> Lists<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Drops every key under `prefixes`, then writes `entries`, under the writer lock, and advances the cache's
    /// epoch, so that no entry read before the drop serves a read and no fill or rollup begun before it is made.
    pub fn drop_prefixes(
        &self,
        ts: u64,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
    ) -> Result<(), PostingError> {
        let _writer = lock(&self.writer);
        self.refuse_if_failed()?;
        let result = self.store.drop_prefixes(ts, prefixes, entries);
        self.bump_epoch();
        if let Err(error) = result {
            tracing::error!(drop_ts = ts, %error, "prefix drop failed; the store must be reopened");
            self.fail_writes(None);
            return Err(error.into());
        }
        Ok(())
    }

    /// Drops every key under `prefixes` at the next version, which it returns, then writes `entries`, as
    /// [`Lists::drop_prefixes`] does. The version is taken under the writer lock and held in flight until the drop is
    /// done, so every read above the watermark waits for the drop, as it waits for a round being written, and sees
    /// none of the dropped data; the watermark then moves to the drop's version. A read at the watermark may see
    /// either side.
    pub fn drop_prefixes_now(
        &self,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
    ) -> Result<u64, PostingError> {
        let _writer = lock(&self.writer);
        self.refuse_if_failed()?;
        let ts = self.hold_next_version();
        let result = self.store.drop_prefixes(ts, prefixes, entries);
        self.bump_epoch();
        self.release_held(ts, result.is_ok());
        if let Err(error) = result {
            tracing::error!(drop_ts = ts, %error, "prefix drop failed; the store must be reopened");
            self.fail_writes(None);
            return Err(error.into());
        }
        Ok(ts)
    }

    /// Drops every key under `prefixes` at `ts`, a version the commit step gave the drop, then writes `entries`, as
    /// [`Lists::drop_prefixes_now`] does: what a drop decided in a round does once the round's batch is written (7.7).
    ///
    /// # Errors
    ///
    /// [`PostingError::StaleVersion`] when `ts` is not above the applied version, writing nothing; the store's error,
    /// which stops writes as [`Lists::drop_prefixes_now`]'s does.
    pub fn drop_prefixes_at(
        &self,
        ts: u64,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
    ) -> Result<(), PostingError> {
        let _writer = lock(&self.writer);
        self.refuse_if_failed()?;
        {
            let mut state = lock(&self.state);
            if ts <= state.applied {
                return Err(PostingError::StaleVersion(ts, state.applied));
            }
            state.in_flight = Some(ts);
        }
        let result = self.store.drop_prefixes(ts, prefixes, entries);
        self.bump_epoch();
        self.release_held(ts, result.is_ok());
        if let Err(error) = result {
            tracing::error!(drop_ts = ts, %error, "prefix drop failed; the store must be reopened");
            self.fail_writes(None);
            return Err(error.into());
        }
        Ok(())
    }

    /// Drops every key at the next version, which it returns, then writes `entries`, holding reads above the watermark
    /// and moving it as [`Lists::drop_prefixes_now`] does.
    pub fn drop_all_now(&self, entries: Vec<Entry>) -> Result<u64, PostingError> {
        let _writer = lock(&self.writer);
        self.refuse_if_failed()?;
        let ts = self.hold_next_version();
        let result = self.store.drop_all(entries);
        self.bump_epoch();
        self.release_held(ts, result.is_ok());
        if let Err(error) = result {
            tracing::error!(drop_ts = ts, %error, "drop of everything failed; the store must be reopened");
            self.fail_writes(None);
            return Err(error.into());
        }
        Ok(ts)
    }

    /// The next version, held in flight for the read gate.
    fn hold_next_version(&self) -> u64 {
        let mut state = lock(&self.state);
        let ts = (state.applied | 1).saturating_add(1);
        state.in_flight = Some(ts);
        ts
    }

    /// Ends the hold of [`Lists::hold_next_version`], moving the watermark to `ts` when the drop was `done`, and wakes
    /// the reads waiting on it.
    fn release_held(&self, ts: u64, done: bool) {
        let mut state = lock(&self.state);
        if state.in_flight == Some(ts) {
            state.in_flight = None;
        }
        if done {
            state.applied = state.applied.max(ts);
        }
        drop(state);
        self.changed.notify_all();
    }

    /// Drops every key, then writes `entries`, as [`Lists::drop_prefixes`] drops a prefix.
    pub fn drop_all(&self, entries: Vec<Entry>) -> Result<(), PostingError> {
        let _writer = lock(&self.writer);
        self.refuse_if_failed()?;
        let result = self.store.drop_all(entries);
        self.bump_epoch();
        if let Err(error) = result {
            tracing::error!(%error, "drop of everything failed; the store must be reopened");
            self.fail_writes(None);
            return Err(error.into());
        }
        Ok(())
    }

    /// Advances the cache's epoch under the writer lock: what a schema change and an index rebuild do, so that
    /// every older entry is a miss and every older fill and rollup is refused.
    pub fn advance_epoch(&self) {
        let _writer = lock(&self.writer);
        self.bump_epoch();
    }

    fn bump_epoch(&self) {
        lock(&self.state).epoch += 1;
    }

    /// Asks the store for discard timestamp `ts`, held at or below the oldest open request.
    pub fn set_discard_ts(&self, ts: u64) -> Result<(), PostingError> {
        let bound = lock(&self.state).ts.discard_bound(ts);
        Ok(self.store.set_discard_ts(bound)?)
    }
}
