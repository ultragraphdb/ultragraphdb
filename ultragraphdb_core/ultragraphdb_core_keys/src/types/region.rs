/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::quad_key_error::QuadKeyError;
use crate::types::keyspace::Keyspace;
use crate::types::quad_key::QuadKey;

/// The key range a key belongs to, the unit a write's reads and writes are reported in: the keyspace, the namespace,
/// the graph and, for the keyspaces whose keys name one, the predicate. The catalog's region is its namespace's. A
/// part of a split list belongs to its list's region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Region {
    keyspace: Keyspace,
    namespace: u64,
    graph: Option<u64>,
    predicate: Option<u64>,
}

impl Region {
    pub(crate) fn new(
        keyspace: Keyspace,
        namespace: u64,
        graph: Option<u64>,
        predicate: Option<u64>,
    ) -> Self {
        Self {
            keyspace,
            namespace,
            graph,
            predicate,
        }
    }

    /// The region of `key`, computed from the key alone.
    pub fn of(key: &[u8]) -> Result<Self, QuadKeyError> {
        QuadKey::parse(key).map(|(parsed, _)| parsed.region())
    }

    pub fn keyspace(&self) -> Keyspace {
        self.keyspace
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn graph(&self) -> Option<u64> {
        self.graph
    }

    pub fn predicate(&self) -> Option<u64> {
        self.predicate
    }
}
