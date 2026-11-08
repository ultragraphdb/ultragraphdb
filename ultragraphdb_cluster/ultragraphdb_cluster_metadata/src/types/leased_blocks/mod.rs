/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod leased_blocks_block_source;

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_blocks::SequenceBlock;

/// The sequence blocks the metadata group leased to one partition, which the partition's store takes through
/// `BlockSource::lease`: its host leases a block from the group ([`MetadataGroup::lease_block`](crate::MetadataGroup)),
/// waits until the lease is durable, and supplies it here, so `lease` returns at once and hands out only durable
/// blocks. The host keeps a block ahead of each type the store allocates, topping up when [`LeasedBlocks::held`]
/// falls to none. Its clones share one queue, as the store and its host share one partition.
#[derive(Debug, Clone, Default)]
pub struct LeasedBlocks {
    // Shared: the store leases from the clone it holds, and its host supplies through another. Each type's queue holds
    // at most `HELD_MAX` blocks.
    blocks: Arc<Mutex<BTreeMap<u16, VecDeque<SequenceBlock>>>>,
}

impl LeasedBlocks {
    /// The most blocks of one type held, 16.
    pub const HELD_MAX: usize = 16;

    /// A partition's queue holding no block.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Supplies `block`, a durable lease of `prefix`'s sequences, after the blocks of that type held already.
    ///
    /// # Errors
    ///
    /// [`ClusterError::full`] when [`LeasedBlocks::HELD_MAX`] blocks of `prefix` are held.
    pub fn supply(&self, prefix: TypePrefix, block: SequenceBlock) -> Result<(), ClusterError> {
        let mut blocks = self.lock();
        let held = blocks.entry(prefix.get()).or_default();
        if held.len() >= Self::HELD_MAX {
            return Err(ClusterError::full(
                "blocks of one type held",
                Self::HELD_MAX,
            ));
        }
        held.push_back(block);
        Ok(())
    }

    /// The blocks of `prefix` held and not yet taken.
    #[must_use]
    pub fn held(&self, prefix: TypePrefix) -> usize {
        self.lock().get(&prefix.get()).map_or(0, VecDeque::len)
    }

    /// The shared queue, a poisoned lock's included: every change to it is made whole before the guard drops.
    pub(crate) fn lock(&self) -> MutexGuard<'_, BTreeMap<u16, VecDeque<SequenceBlock>>> {
        self.blocks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
