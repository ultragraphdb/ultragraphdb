/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_keys::Region;

use crate::types::reads::Reads;

/// A changeset applied: its commit timestamp, the regions of every key it wrote and read, from which stage 5 builds
/// read summaries, the number of lists it read from the store, and its read set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    commit_ts: u64,
    written: BTreeSet<Region>,
    read: BTreeSet<Region>,
    lists_read: usize,
    reads: Reads,
}

impl Applied {
    pub(crate) fn new(
        commit_ts: u64,
        written: BTreeSet<Region>,
        read: BTreeSet<Region>,
        lists_read: usize,
        reads: Reads,
    ) -> Self {
        Self {
            commit_ts,
            written,
            read,
            lists_read,
            reads,
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

    pub fn lists_read(&self) -> usize {
        self.lists_read
    }

    /// What the changeset's execution read: the read set its commit is validated by.
    pub fn reads(&self) -> &Reads {
        &self.reads
    }
}
