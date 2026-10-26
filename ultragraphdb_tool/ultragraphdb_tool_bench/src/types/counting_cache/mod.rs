/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod counting_cache_list_cache;

use std::sync::Arc;

use crate::types::read_counts::ReadCounts;

/// A posting cache with every lookup counted, and whether it found an entry: a list read looks the cache up once, and
/// a commit once per key it writes, to update the key's entry. An entry a read finds serves it unless a drop changed
/// the epoch or the read is below the entry's timestamp, which a run's reads, at the newest timestamp and without
/// drops, never are; an entry a commit finds is updated. The entries found are the run's hits.
#[derive(Debug)]
pub struct CountingCache<C> {
    inner: C,
    counts: Arc<ReadCounts>,
}

impl<C> CountingCache<C> {
    pub fn new(inner: C, counts: Arc<ReadCounts>) -> Self {
        Self { inner, counts }
    }
}
