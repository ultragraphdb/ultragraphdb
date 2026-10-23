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
