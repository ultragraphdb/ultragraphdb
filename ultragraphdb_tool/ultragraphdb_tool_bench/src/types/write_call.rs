/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// One write call to the store: its time, its entries, whether it was a commit batch (it records an applied
/// timestamp, which rollup batches do not), and fjall's write pressure on the data keyspace after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteCall {
    nanos: u64,
    entries: usize,
    commit: bool,
    l0_tables: usize,
    sealed_memtables: usize,
}

impl WriteCall {
    pub fn new(
        nanos: u64,
        entries: usize,
        commit: bool,
        l0_tables: usize,
        sealed_memtables: usize,
    ) -> Self {
        Self {
            nanos,
            entries,
            commit,
            l0_tables,
            sealed_memtables,
        }
    }

    pub fn nanos(&self) -> u64 {
        self.nanos
    }

    pub fn entries(&self) -> usize {
        self.entries
    }

    pub fn commit(&self) -> bool {
        self.commit
    }

    pub fn l0_tables(&self) -> usize {
        self.l0_tables
    }

    pub fn sealed_memtables(&self) -> usize {
        self.sealed_memtables
    }

    /// Whether fjall's stall or halt could have run in the call: 20 or more level-0 tables, or 4 or more sealed
    /// memtables, after it.
    pub fn under_pressure(&self) -> bool {
        self.l0_tables >= 20 || self.sealed_memtables >= 4
    }
}
