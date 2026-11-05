/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

use crate::errors::quad_key_error::QuadKeyError;
use crate::types::graph::Graph;
use crate::types::keyspace::Keyspace;
use crate::types::quad_key::QuadKey;

/// The key range a key belongs to, the unit a write's reads and writes are reported in: the keyspace, the tenant,
/// the graph for the keyspaces whose keys name one and, for those whose keys name one too, the predicate. The
/// catalog's, the marks' and the referrers' regions are their tenant's. A part of a split list belongs to its
/// list's region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Region {
    keyspace: Keyspace,
    tenant: u64,
    graph: Option<Graph>,
    predicate: Option<SystemId>,
}

impl Region {
    pub(crate) fn new(
        keyspace: Keyspace,
        tenant: u64,
        graph: Option<Graph>,
        predicate: Option<SystemId>,
    ) -> Self {
        Self {
            keyspace,
            tenant,
            graph,
            predicate,
        }
    }

    /// The region of `key`, computed from the key alone.
    ///
    /// # Errors
    ///
    /// The refusals of [`crate::QuadKey::parse`].
    pub fn of(key: &[u8]) -> Result<Self, QuadKeyError> {
        QuadKey::parse(key).map(|(parsed, _)| parsed.region())
    }

    /// The keyspace.
    #[must_use]
    pub fn keyspace(&self) -> Keyspace {
        self.keyspace
    }

    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The graph, when the keyspace's keys name one.
    #[must_use]
    pub fn graph(&self) -> Option<Graph> {
        self.graph
    }

    /// The predicate, when the keyspace's keys name one.
    #[must_use]
    pub fn predicate(&self) -> Option<SystemId> {
        self.predicate
    }
}
