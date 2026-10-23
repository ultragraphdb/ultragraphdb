/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::mem;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use fjall::Slice;

use crate::utils::lock::lock;

/// What the compaction filter reads, shared with the store: atomics, because fjall starts compactions inside `open`,
/// before the store has read its discard timestamp from the metadata keyspace. Until it has, the discard timestamp
/// is 0 and the filter collects nothing.
#[derive(Debug, Default)]
pub struct GcSettings {
    /// The discard timestamp in effect: the one asked for, held at or below the oldest open snapshot.
    discard_ts: AtomicU64,
    /// The items compactions kept because they could not decode them.
    undecodable: AtomicU64,
    /// The stored keys compactions collected and the store has not yet removed through a journaled batch. fjall
    /// replays its active journal whole at open, flushed items included, so a version a compaction filter destroyed
    /// returns at the next open unless a removal of it follows in the journal.
    collected: Mutex<Vec<Slice>>,
}

impl GcSettings {
    pub fn discard_ts(&self) -> u64 {
        self.discard_ts.load(Ordering::Acquire)
    }

    pub fn set_discard_ts(&self, ts: u64) {
        self.discard_ts.store(ts, Ordering::Release);
    }

    pub fn undecodable(&self) -> u64 {
        self.undecodable.load(Ordering::Acquire)
    }

    pub fn add_undecodable(&self, count: u64) {
        self.undecodable.fetch_add(count, Ordering::AcqRel);
    }

    pub fn add_collected(&self, keys: Vec<Slice>) {
        if !keys.is_empty() {
            lock(&self.collected).extend(keys);
        }
    }

    /// How many collected stored keys wait for their removal.
    pub fn collected_len(&self) -> usize {
        lock(&self.collected).len()
    }

    pub fn take_collected(&self) -> Vec<Slice> {
        mem::take(&mut *lock(&self.collected))
    }
}
