/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::read_set::ReadSet;

/// The buffers a worker lends each transaction it runs and gets back when the transaction ends: the read set and the
/// write-set record's bytes. A transaction clears them and keeps their capacity, so a worker in its steady state
/// allocates neither.
#[derive(Debug, Clone, Default)]
pub struct TxnBuffers {
    read_set: ReadSet,
    record: Vec<u8>,
}

impl TxnBuffers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Buffers holding a read set of `read_bytes` over `keys` keys and a record of `record_bytes` before they grow.
    pub fn with_capacity(read_bytes: usize, keys: usize, record_bytes: usize) -> Self {
        Self {
            read_set: ReadSet::with_capacity(read_bytes, keys),
            record: Vec::with_capacity(record_bytes),
        }
    }

    pub(crate) fn from_parts(read_set: ReadSet, record: Vec<u8>) -> Self {
        Self { read_set, record }
    }

    pub(crate) fn into_parts(self) -> (ReadSet, Vec<u8>) {
        (self.read_set, self.record)
    }

    /// The read set's buffer capacities, as [`ReadSet::capacity`] gives them.
    pub fn read_set_capacity(&self) -> (usize, usize) {
        self.read_set.capacity()
    }

    /// The record buffer's capacity, in bytes.
    pub fn record_capacity(&self) -> usize {
        self.record.capacity()
    }
}
