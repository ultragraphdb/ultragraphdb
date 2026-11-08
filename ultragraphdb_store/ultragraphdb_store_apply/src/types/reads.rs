/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Keyspace, key_prefix};
use ultragraphdb_traits_store::Scan;

/// What a changeset's execution read, the read set a commit is validated by: the lists it read whole and the marks it
/// read (partitions, "A commit under a registration reply reads its mark"), the statements it tested, the key ranges it scanned, the terms it found absent, by ID, and its tenant's catalog range, which
/// every write of the tenant reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reads {
    tenant: u64,
    lists: Vec<Vec<u8>>,
    statements: Vec<(Vec<u8>, Vec<u8>)>,
    ranges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    absent: Vec<SystemId>,
}

impl Reads {
    pub(crate) fn new(
        tenant: u64,
        lists: Vec<Vec<u8>>,
        statements: Vec<(Vec<u8>, Vec<u8>)>,
        ranges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
        absent: Vec<SystemId>,
    ) -> Self {
        Self {
            tenant,
            lists,
            statements,
            ranges,
            absent,
        }
    }

    /// The tenant the reads were made in.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The keys of the lists read whole and of the marks read, in key order: a write to one conflicts.
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

    /// The terms found absent, by ID in ID order: no statement of the term in the store. No dictionary key is among
    /// them.
    #[must_use]
    pub fn absent(&self) -> &[SystemId] {
        &self.absent
    }

    /// The tenant's catalog range, from its start, inclusive, to its end, exclusive.
    pub fn catalog(&self) -> (Vec<u8>, Option<Vec<u8>>) {
        let start = key_prefix(Keyspace::Catalog, false, self.tenant, None, None);
        let end = Scan::prefix(start.clone()).end().map(<[u8]>::to_vec);
        (start, end)
    }
}
