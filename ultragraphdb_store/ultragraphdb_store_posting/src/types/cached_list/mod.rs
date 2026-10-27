/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use crate::types::list::List;

/// An entry of the posting cache: an immutable list, the timestamp it is current from, and the cache's epoch when it
/// was written. Dgraph's `CachePL`, with the epoch the port adds.
///
/// The entry serves a read at `t` when its timestamp is at or below `t`, its epoch is the cache's, and `t` is at or
/// above the list's complete version. A commit to the key replaces or removes the entry under the writer lock
/// before the commit is applied, so an entry present when a read has passed the read gate reflects every commit to
/// its key at or below the read's timestamp.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedList {
    list: Arc<List>,
    ts: u64,
    epoch: u64,
}

impl CachedList {
    pub fn new(list: Arc<List>, ts: u64, epoch: u64) -> Self {
        Self { list, ts, epoch }
    }

    pub fn list(&self) -> &Arc<List> {
        &self.list
    }

    /// The timestamp from which the entry is current.
    pub fn ts(&self) -> u64 {
        self.ts
    }

    /// The cache's epoch when the entry was written.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Whether the entry serves a read at `read_ts` while the cache is at `epoch`.
    pub fn serves(&self, read_ts: u64, epoch: u64) -> bool {
        self.epoch == epoch && self.ts <= read_ts && self.list.min_ts() <= read_ts
    }

    /// The entry's weight for a cache bounded in bytes: the key, and the encoded sizes of the complete list, its
    /// parts and its deltas, plus a fixed overhead for the entry, at most `u32::MAX`. Dgraph's cost is
    /// `ApproximateSize`, which counts Go's struct layouts.
    pub fn weight(&self) -> u32 {
        const OVERHEAD: usize = 64;
        let list = &self.list;
        let parts: usize = list.parts().iter().map(|p| p.encoded_len()).sum();
        let deltas: usize = list
            .mutation()
            .committed()
            .values()
            .map(|d| d.encoded_len())
            .sum();
        let bytes = OVERHEAD + list.key().len() + list.plist().encoded_len() + parts + deltas;
        u32::try_from(bytes).unwrap_or(u32::MAX)
    }
}
