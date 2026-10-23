/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/mvcc.go (pooledKeys, incrRollupi.addKeyToBatch, the batches Process takes and
 * its deduplication). Changed for ultragraphdb: translated to Rust; the batch pools and channels are queues under
 * one lock; the time a key was last rolled up is the caller's clock in seconds, where Dgraph reads the wall clock,
 * and a key never rolled up is due, where Dgraph's check relies on Unix time being large; keys are deduplicated by
 * their 64-bit farm fingerprint, where Dgraph uses `z.MemHash`.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::{HashMap, VecDeque};

use ultragraphdb_common_utils::go_farm::fingerprint64;

/// The keys a batch holds before it is handed to the worker.
pub const ROLLUP_BATCH: usize = 16;
/// The full batches a priority's queue holds; a batch beyond them is dropped, as Dgraph's channel drops it.
pub const ROLLUP_QUEUE: usize = 16;
/// The seconds within which a key rolled up is not rolled up again.
pub const ROLLUP_DEDUP_SECS: u64 = 10;
/// The seconds between purges of the deduplication map.
const CLEANUP_SECS: u64 = 300;

/// The keys waiting to be rolled up, in two priorities, 0 high and 1 low: a batch fills to [`ROLLUP_BATCH`] keys
/// and then waits in its priority's queue; Dgraph's `incrRollupi` without its goroutine.
#[derive(Debug, Clone, Default)]
pub(crate) struct RollupQueues {
    pending: [Vec<Vec<u8>>; 2],
    full: [VecDeque<Vec<Vec<u8>>>; 2],
    rolled: HashMap<u64, u64>,
    last_cleanup: u64,
}

impl RollupQueues {
    /// Adds `key` to the batch of `priority`; a full batch goes to its queue, or is dropped when the queue is full.
    pub fn add_key(&mut self, key: Vec<u8>, priority: usize) {
        let priority = priority.min(1);
        self.pending[priority].push(key);
        if self.pending[priority].len() < ROLLUP_BATCH {
            return;
        }
        let batch = std::mem::take(&mut self.pending[priority]);
        if self.full[priority].len() < ROLLUP_QUEUE {
            self.full[priority].push_back(batch);
        }
    }

    /// The next batch to roll up: a full high-priority batch, then a full low-priority one, then the high-priority
    /// keys gathered so far, which Dgraph's forced tick takes every 500 ms.
    pub fn take_batch(&mut self) -> Option<Vec<Vec<u8>>> {
        self.full[0]
            .pop_front()
            .or_else(|| self.full[1].pop_front())
            .or_else(|| (!self.pending[0].is_empty()).then(|| std::mem::take(&mut self.pending[0])))
    }

    /// Whether `key` is due at `now_secs`, recording it rolled up then when it is: a key rolled up in the last
    /// [`ROLLUP_DEDUP_SECS`] seconds is not.
    pub fn due(&mut self, key: &[u8], now_secs: u64) -> bool {
        if now_secs.saturating_sub(self.last_cleanup) >= CLEANUP_SECS {
            self.rolled
                .retain(|_, at| now_secs.saturating_sub(*at) < ROLLUP_DEDUP_SECS);
            self.last_cleanup = now_secs;
        }
        let hash = fingerprint64(key);
        let due = self
            .rolled
            .get(&hash)
            .is_none_or(|&at| now_secs.saturating_sub(at) >= ROLLUP_DEDUP_SECS);
        if due {
            self.rolled.insert(hash, now_secs);
        }
        due
    }

    /// How many keys wait, in batches and in the batches being filled.
    pub fn waiting(&self) -> usize {
        self.pending.iter().map(Vec::len).sum::<usize>()
            + self
                .full
                .iter()
                .flat_map(|q| q.iter())
                .map(Vec::len)
                .sum::<usize>()
    }
}
