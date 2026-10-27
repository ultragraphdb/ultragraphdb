/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{Keyspace, key_prefix};
use ultragraphdb_traits_store::Scan;

/// What a changeset's execution read, the read set a commit is validated by: the lists it read whole, the statements
/// it tested, the key ranges it scanned, the dictionary keys it found absent, and its namespace's catalog range, which
/// every write of the namespace reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reads {
    namespace: u64,
    lists: Vec<Vec<u8>>,
    statements: Vec<(Vec<u8>, Vec<u8>)>,
    ranges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    absent: Vec<Vec<u8>>,
}

impl Reads {
    pub(crate) fn new(
        namespace: u64,
        lists: Vec<Vec<u8>>,
        statements: Vec<(Vec<u8>, Vec<u8>)>,
        ranges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
        absent: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            namespace,
            lists,
            statements,
            ranges,
            absent,
        }
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The keys of the lists read whole, in key order.
    pub fn lists(&self) -> &[Vec<u8>] {
        &self.lists
    }

    /// The statements tested, by list key and object encoding, in key order.
    pub fn statements(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.statements
    }

    /// The key ranges scanned, each from its start, inclusive, to its end, exclusive, or to the last key.
    pub fn ranges(&self) -> &[(Vec<u8>, Option<Vec<u8>>)] {
        &self.ranges
    }

    /// The dictionary keys found absent, in key order.
    pub fn absent(&self) -> &[Vec<u8>] {
        &self.absent
    }

    /// The namespace's catalog range, from its start, inclusive, to its end, exclusive.
    pub fn catalog(&self) -> (Vec<u8>, Option<Vec<u8>>) {
        let start = key_prefix(Keyspace::Catalog, false, self.namespace, None, None);
        let end = Scan::prefix(start.clone()).end().map(<[u8]>::to_vec);
        (start, end)
    }
}
