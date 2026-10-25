/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/mvcc.go (CommitToDisk, UpdateCachedKeys, updateItemInCache), posting/writer.go
 * (TxnWriter) and posting/oracle.go (ProcessDelta's rollup keys). Changed for ultragraphdb: translated to Rust;
 * a batch of requests is written as one store batch, each request at the commit timestamp the writer gives it as
 * it writes, with the applied timestamp, where Dgraph commits each Badger transaction on its own and splits one on
 * `ErrTxnTooBig`; the cache entries of the lists written are replaced or removed under the writer lock before the
 * commit is applied; `maxVersions`, the pending-transaction table and the oracle's waiters are not ported.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::Arc;

use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_traits_store::{Entry, ListCache, Store, WriteBatch};

use crate::errors::posting_error::PostingError;
use crate::types::batch::Batch;
use crate::types::cached_list::CachedList;
use crate::types::lists::Lists;
use crate::types::request::Request;
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

    /// Commits every request of `batch`, in order: the commit timestamps it took, one per request.
    pub fn commit(&self, batch: Batch<'_, S, C>) -> Result<Vec<u64>, PostingError> {
        self.commit_with(batch, |_| Ok::<_, PostingError>(()))?
            .into_iter()
            .collect()
    }

    /// Commits `batch` as [`Lists::commit`] does, first calling `check` with each request under the writer lock;
    /// a request `check` refuses is left out and writes nothing, and its result is the refusal. The requests left
    /// in are written as one store batch, each at a fresh commit timestamp, recording the last as applied; the
    /// cache entries of their lists are then replaced or removed, and the commit is applied. An entry a request adds
    /// at its own version keeps it; a request marked to advance the epoch advances it before the commit is applied;
    /// the rebuild records a request names are removed in the same store batch.
    ///
    /// # Errors
    ///
    /// The store's error when the batch cannot be written, after which every write is refused until the store is
    /// reopened, as is every read at or above the batch's first timestamp.
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
        let kept = checked.iter().filter(|c| c.is_ok()).count();
        if kept == 0 {
            return Ok(checked.into_iter().map(|c| c.map(|()| 0)).collect());
        }

        let (first, epoch) = {
            let mut state = lock(&self.state);
            let first = state.ts.peek();
            (0..kept).for_each(|_| {
                state.ts.fresh();
            });
            state.in_flight = Some(first);
            (first, state.epoch)
        };
        let last = first + kept as u64 - 1;

        let advance_epoch = requests
            .iter()
            .zip(&checked)
            .any(|(request, c)| c.is_ok() && request.advances_epoch());
        let mut store_batch = WriteBatch::new();
        let written: Vec<Written<'_, '_, S, C>> = requests
            .iter()
            .zip(&checked)
            .filter(|(_, c)| c.is_ok())
            .zip(first..)
            .map(|((request, _), commit_ts)| {
                for (key, delta) in request.written() {
                    store_batch.push(Entry::put(
                        key.to_vec(),
                        commit_ts,
                        BIT_DELTA_POSTING,
                        marshal(delta),
                    ));
                }
                for entry in request.entries() {
                    store_batch.push(at_version(entry, commit_ts));
                }
                for entry in request.versioned_entries() {
                    store_batch.push(entry.clone());
                }
                for id in request.removed_records() {
                    store_batch.remove_rebuild_record(*id);
                }
                (commit_ts, request)
            })
            .collect();
        store_batch.set_applied_ts(last);

        if let Err(error) = self.store.apply(store_batch, self.config.persist()) {
            tracing::error!(first_commit_ts = first, last_commit_ts = last, %error, "commit batch failed; the store must be reopened");
            self.fail_writes(Some(first));
            return Err(error.into());
        }

        for (ts, request) in &written {
            for (key, delta) in request.written() {
                self.update_cached(key, *ts, delta, epoch);
            }
        }
        {
            let mut state = lock(&self.state);
            if !state.fills.is_empty() {
                for (ts, request) in &written {
                    for key in request.written_keys() {
                        state.touched.insert(key.to_vec(), *ts);
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
        for (_, request) in &written {
            for key in request.written_keys() {
                self.add_rollup_key(key.to_vec(), 1);
                self.add_rollup_key(key.to_vec(), 0);
            }
        }

        let mut ts = first;
        Ok(checked
            .into_iter()
            .map(|c| {
                c.map(|()| {
                    let this = ts;
                    ts += 1;
                    this
                })
            })
            .collect())
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

/// A request's commit timestamp and the request, whose deltas it wrote.
type Written<'r, 'a, S, C> = (u64, &'r Request<'a, S, C>);

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
