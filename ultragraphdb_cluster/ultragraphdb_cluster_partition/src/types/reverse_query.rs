/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::{Epoch, SystemId};
use ultragraphdb_core_keys::Graph;

/// A reverse read of `object` through `predicate` in `graph` of `namespace` at the closed epoch `epoch`, `object`
/// homed in `home` (quad-reads, "A reverse read asks the object's referrers").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReverseQuery {
    home: PartitionId,
    namespace: u64,
    graph: Graph,
    predicate: SystemId,
    object: SystemId,
    epoch: Epoch,
}

impl ReverseQuery {
    /// The reverse read of `object`, homed in `home`, through `predicate` in `graph` of `namespace` at `epoch`.
    pub fn new(
        home: PartitionId,
        namespace: u64,
        graph: Graph,
        predicate: SystemId,
        object: SystemId,
        epoch: Epoch,
    ) -> Self {
        Self {
            home,
            namespace,
            graph,
            predicate,
            object,
            epoch,
        }
    }

    /// The object's home.
    pub fn home(&self) -> PartitionId {
        self.home
    }

    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The graph.
    pub fn graph(&self) -> Graph {
        self.graph
    }

    /// The predicate.
    pub fn predicate(&self) -> SystemId {
        self.predicate
    }

    /// The object.
    pub fn object(&self) -> SystemId {
        self.object
    }

    /// The closed epoch the read is at.
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }
}
