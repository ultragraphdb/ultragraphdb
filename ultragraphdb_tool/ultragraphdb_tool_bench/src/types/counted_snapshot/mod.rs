/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod counted_snapshot_store_snapshot;

use std::sync::Arc;

use ultragraphdb_store_kv::{FjallScan, FjallSnapshot};

use crate::types::read_counts::ReadCounts;

/// A fjall snapshot with every read of a key's versions counted, and those that found none: how a list is read from
/// the store, by a cache miss or a rollup.
pub struct CountedSnapshot {
    inner: FjallSnapshot,
    counts: Arc<ReadCounts>,
}

impl CountedSnapshot {
    pub fn new(inner: FjallSnapshot, counts: Arc<ReadCounts>) -> Self {
        Self { inner, counts }
    }
}

/// A scan of the store, counted as empty when it is a read of a key's versions and yields nothing.
pub struct CountedScan {
    inner: FjallScan,
    /// Where to count an empty read; `None` once the scan has yielded, and for other scans.
    empty: Option<Arc<ReadCounts>>,
}

impl Iterator for CountedScan {
    type Item = <FjallScan as Iterator>::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.inner.next();
        if let Some(counts) = self.empty.take()
            && item.is_none()
        {
            counts.empty_read();
        }
        item
    }
}
