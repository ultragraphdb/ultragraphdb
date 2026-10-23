/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, HashMap};

use crate::types::timestamps::Timestamps;

/// What the read gate, the cache's rule and the rollups share, under one lock.
#[derive(Debug)]
pub(crate) struct State {
    pub ts: Timestamps,
    /// The highest commit timestamp applied: written, and its cache updates made.
    pub applied: u64,
    /// The first commit timestamp of the batch being written.
    pub in_flight: Option<u64>,
    /// The first commit timestamp of a batch whose write failed: reads at or above it are refused, as nothing tells
    /// whether the batch survives the reopening.
    pub unapplied_from: Option<u64>,
    /// Whether a write failed, which stops every later write until the store is reopened.
    pub write_failed: bool,
    /// The cache's epoch, advanced by drops, schema changes and rebuilds.
    pub epoch: u64,
    /// The applied timestamps of the cache fills under way, with how many there are of each.
    pub fills: BTreeMap<u64, usize>,
    /// While fills are under way, the newest commit timestamp of each key written.
    pub touched: HashMap<Vec<u8>, u64>,
    /// How many callers have paused the rollups.
    pub paused: usize,
}

impl State {
    pub fn new(applied: u64) -> Self {
        Self {
            ts: Timestamps::new(applied),
            applied,
            in_flight: None,
            unapplied_from: None,
            write_failed: false,
            epoch: 0,
            fills: BTreeMap::new(),
            touched: HashMap::new(),
            paused: 0,
        }
    }
}
