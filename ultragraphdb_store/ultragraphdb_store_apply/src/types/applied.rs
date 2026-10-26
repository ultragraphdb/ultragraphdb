/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_keys::Region;

/// A changeset applied: its commit timestamp, and the regions of every key it wrote and read, from which stage 5 builds
/// read summaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    commit_ts: u64,
    written: BTreeSet<Region>,
    read: BTreeSet<Region>,
}

impl Applied {
    pub(crate) fn new(commit_ts: u64, written: BTreeSet<Region>, read: BTreeSet<Region>) -> Self {
        Self {
            commit_ts,
            written,
            read,
        }
    }

    pub fn commit_ts(&self) -> u64 {
        self.commit_ts
    }

    pub fn written(&self) -> &BTreeSet<Region> {
        &self.written
    }

    pub fn read(&self) -> &BTreeSet<Region> {
        &self.read
    }
}
