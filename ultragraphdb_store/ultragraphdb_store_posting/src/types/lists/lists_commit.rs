/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/mvcc.go (CommitToDisk, UpdateCachedKeys, updateItemInCache), posting/writer.go
 * (TxnWriter) and posting/oracle.go (ProcessDelta's rollup keys). Changed for ultragraphdb: translated to Rust;
 * a round of transactions, or a batch of requests, is written as one store batch, each at its commit version, with the
 * applied version, where Dgraph commits each Badger transaction on its own and splits one on `ErrTxnTooBig`; the cache entries of the lists written are replaced or removed under the writer lock before the
 * commit is applied; `maxVersions`, the pending-transaction table and the oracle's waiters are not ported.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::Arc;

use ultragraphdb_core_codec::PostingList;
use ultragraphdb_traits_store::{Entry, ListCache, PersistMode, Store, WriteBatch};

use crate::errors::posting_error::PostingError;
use crate::types::batch::Batch;
use crate::types::cached_list::CachedList;
use crate::types::lists::Lists;
use crate::types::request::Request;
use crate::types::writes::Writes;
use crate::utils::constants::BIT_DELTA_POSTING;
use crate::utils::lock::lock;
use crate::utils::posting_codec::marshal;

impl<S, C> Lists<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// A batch of requests to commit together.
    pub fn batch(&self) -> Batch<'_, S, C> {
        Batch::new(self)
    }

    /// A request on its own, reading at `read_ts`.
    pub fn request(&self, read_ts: u64) -> Request<'_, S, C> {
        Request::new(self, read_ts, Default::default())
    }

    /// Commits every request of `batch`, in order: the commit versions it took, one per request.
    pub fn commit(&self, batch: Batch<'_, S, C>) -> Result<Vec<u64>, PostingError> {
        self.commit_with(batch, |_| Ok::<_, PostingError>(()))?
            .into_iter()
            .collect()
    }

    /// Commits `batch` as [`Lists::commit`] does, first calling `check` with each request under the writer lock;
    /// a request `check` refuses is left out and writes nothing, and its result is the refusal. The requests left
    /// in are written as one store batch in the configured persist mode, each at the next even version above the one
    /// before it, the first above the watermark, recording the last as applied; the cache entries of their lists are
    /// then replaced or removed, and the watermark passes them. An entry a request adds at its own version keeps it; a
    /// request marked to advance the epoch advances it before the watermark moves; the rebuild records a request names
    /// are removed in the same store batch.
    ///
    /// # Errors
    ///
    /// The store's error when the batch cannot be written, after which every write is refused until the store is
    /// reopened, as is every read at or above the batch's first version.
    pub fn commit_with<E, F>(
        &self,
        batch: Batch<'_, S, C>,
        mut check: F,
    ) -> Result<Vec<Result<u64, E>>, PostingError>
    where
        F: FnMut(&mut Request<'_, S, C>) -> Result<(), E>,
    {
        let _writer = lock(&self.writer);
        self.refuse_if_failed()?;
        let mut requests = batch.into_requests();
        let checked: Vec<Result<(), E>> = requests.iter_mut().map(&mut check).collect();
        let mut version = next_even(lock(&self.state).applied);
        let mut kept: Vec<(u64, Writes)> = Vec::new();
        let mut results = Vec::with_capacity(checked.len());
        for (request, check) in requests.into_iter().zip(checked) {
            match check {
                Ok(()) => {
                    kept.push((version, request.into_writes()));
                    results.push(Ok(version));
                    version = version.saturating_add(2);
                }
                Err(e) => results.push(Err(e)),
            }
        }
        let round: Vec<(u64, &Writes)> = kept.iter().map(|(v, w)| (*v, w)).collect();
        self.write_round(&round, self.config.persist())?;
        Ok(results)
    }

    /// The smallest even version above the watermark: the commit version of a write the commit step does not order,
    /// a batch's or a drop's, and of a round's first transaction when nothing else orders it. Commit versions are
    /// even so that a rollup, written one above the newest commit it folds, never takes one.
    pub fn next_version(&self) -> u64 {
        next_even(lock(&self.state).applied)
    }

    /// Writes `round`, the admitted transactions of one round, each with its commit version, as one store batch with
    /// [`PersistMode::Buffer`], recording the round's last version as applied, without a sync: the log is the
    /// durability point. The cache entries of the lists written are then replaced or removed, the rollups of their
    /// keys queued, and only then does the watermark pass the round. An empty round writes nothing.
    ///
    /// # Errors
    ///
    /// [`PostingError::StaleVersion`] when a version is not above the one before it, the watermark for the first,
    /// and nothing is written; the store's error when the batch cannot be written, after which every write is refused
    /// until the store is reopened, as is every read at or above the round's first version.
    pub fn commit_round(&self, round: &[(u64, &Writes)]) -> Result<(), PostingError> {
        let _writer = lock(&self.writer);
        self.refuse_if_failed()?;
        let mut after = lock(&self.state).applied;
        for (version, _) in round {
            if *version <= after {
                return Err(PostingError::StaleVersion(*version, after));
            }
            after = *version;
        }
        self.write_round(round, PersistMode::Buffer)
    }

    /// Writes `round`, its versions checked, under the writer lock the caller holds, in `persist` mode.
    fn write_round(
        &self,
        round: &[(u64, &Writes)],
        persist: PersistMode,
    ) -> Result<(), PostingError> {
        let (Some(&(first, _)), Some(&(last, _))) = (round.first(), round.last()) else {
            return Ok(());
        };
        let epoch = {
            let mut state = lock(&self.state);
            state.in_flight = Some(first);
            state.epoch
        };
        let advance_epoch = round.iter().any(|(_, writes)| writes.advances_epoch());
        let mut store_batch = WriteBatch::new();
        for (version, writes) in round {
            for (key, delta) in writes.deltas() {
                store_batch.push(Entry::put(
                    key.clone(),
                    *version,
                    BIT_DELTA_POSTING,
                    marshal(delta),
                ));
            }
            for entry in writes.entries() {
                store_batch.push(at_version(entry, *version));
            }
            for entry in writes.versioned_entries() {
                store_batch.push(entry.clone());
            }
            for id in writes.removed_records() {
                store_batch.remove_rebuild_record(*id);
            }
            for entry in writes.versionless() {
                if let Some(value) = entry.value() {
                    store_batch.put_versionless(entry.key().to_vec(), value.to_vec());
                }
            }
        }
        store_batch.set_applied_ts(last);

        if let Err(error) = self.store.apply(store_batch, persist) {
            tracing::error!(first_commit_ts = first, last_commit_ts = last, %error, "commit batch failed; the store must be reopened");
            self.fail_writes(Some(first));
            return Err(error.into());
        }

        for (version, writes) in round {
            for (key, delta) in writes.deltas() {
                self.update_cached(key, *version, delta, epoch);
            }
        }
        {
            let mut state = lock(&self.state);
            if !state.fills.is_empty() {
                for (version, writes) in round {
                    for key in writes.written_keys() {
                        state.touched.insert(key.to_vec(), *version);
                    }
                }
            }
            state.applied = state.applied.max(last);
            state.in_flight = None;
            if advance_epoch {
                state.epoch += 1;
            }
        }
        self.changed.notify_all();

        // CommitToDisk queues the keys at low priority, and ProcessDelta at high priority once Zero has decided.
        for (_, writes) in round {
            for key in writes.written_keys() {
                self.add_rollup_key(key.to_vec(), 1);
                self.add_rollup_key(key.to_vec(), 0);
            }
        }
        Ok(())
    }

    /// Replaces the cache entry of `key` with its list after `delta`'s commit at `commit_ts`, or removes an entry of
    /// an older epoch: Dgraph's `updateItemInCache`.
    fn update_cached(&self, key: &[u8], commit_ts: u64, delta: &PostingList, epoch: u64) {
        match self.cache.get(key) {
            Some(entry) if entry.epoch() == epoch => {
                let list = entry.list().committed(commit_ts, delta.clone());
                self.cache.insert(
                    key.to_vec(),
                    CachedList::new(Arc::new(list), commit_ts, epoch),
                );
            }
            Some(_) => self.cache.remove(key),
            None => {}
        }
    }
}

/// The smallest even version above `version`, saturating at `u64::MAX`, which a round then refuses as stale.
fn next_even(version: u64) -> u64 {
    (version | 1).saturating_add(1)
}

/// `entry` written at `version`.
fn at_version(entry: &Entry, version: u64) -> Entry {
    let written = match entry.value() {
        Some(value) => Entry::put(entry.key().to_vec(), version, entry.meta(), value.to_vec()),
        None => Entry::deletion(entry.key().to_vec(), version),
    };
    if entry.discard() {
        written.with_discard()
    } else {
        written
    }
}
