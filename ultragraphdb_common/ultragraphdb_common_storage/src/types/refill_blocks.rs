/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::mpsc::UnboundedSender;
use ultragraphdb_cluster_metadata::LeasedBlocks;
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock};

/// A partition's sequence blocks: what its core leased from the metadata group, handed out at once, and a note to the
/// core of each type taken, which leases the next ahead of need. The queue's lock is taken once per block, on the one
/// core that holds the partition, so it is never contended.
#[derive(Debug, Clone)]
pub struct RefillBlocks {
    leased: LeasedBlocks,
    wanted: UnboundedSender<TypePrefix>,
}

impl RefillBlocks {
    /// The blocks `leased` holds, noting each type taken on `wanted`.
    #[must_use]
    pub fn new(leased: LeasedBlocks, wanted: UnboundedSender<TypePrefix>) -> Self {
        Self { leased, wanted }
    }
}

impl BlockSource for RefillBlocks {
    fn lease(&self, prefix: TypePrefix) -> Option<SequenceBlock> {
        // A core gone is a process stopping, which allocates nothing more.
        drop(self.wanted.unbounded_send(prefix));
        self.leased.lease(prefix)
    }
}
