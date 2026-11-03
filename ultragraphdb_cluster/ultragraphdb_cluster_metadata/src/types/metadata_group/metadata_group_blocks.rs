/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId};
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_traits_blocks::SequenceBlock;
use ultragraphdb_traits_log::{OrderedLog, Position};

use crate::types::metadata_command::MetadataCommand;
use crate::types::metadata_group::MetadataGroup;

impl<L: OrderedLog> MetadataGroup<L> {
    /// Leases `partition` the next block of `size` of `prefix`'s sequences, beginning at the type's high-water mark,
    /// which rises past it; returns the block and the position it was appended at. The block is answered once a sync
    /// makes that position durable, so no block is leased twice across a crash.
    ///
    /// # Errors
    ///
    /// [`ClusterError::block_size`] naming `size` and `block_size_max` when `size` is 0 or above it;
    /// [`ClusterError::sequences_spent`] when the block would reach past 2^64 − 1; [`ClusterError::log`]. Nothing is
    /// appended on a refusal.
    pub fn lease_block(
        &mut self,
        prefix: TypePrefix,
        partition: PartitionId,
        size: u64,
    ) -> Result<(SequenceBlock, Position), ClusterError> {
        let max = self.config.block_size_max();
        if size == 0 || size > max {
            return Err(ClusterError::block_size(size, max));
        }
        let start = self.head.next_sequence(prefix);
        let spent = || ClusterError::sequences_spent(prefix.get());
        let end = start.checked_add(size).ok_or_else(spent)?;
        let block = SequenceBlock::new(start, end).ok_or_else(spent)?;
        let position = self.append(MetadataCommand::LeaseBlock {
            prefix,
            partition,
            start,
            end,
        })?;
        Ok((block, position))
    }

    /// Initialises the placement map at the cluster's first start: every namespace run split evenly over the partitions
    /// 1 to `partitions`, each led by `leader`; returns the position it was appended at.
    ///
    /// # Errors
    ///
    /// [`ClusterError::full`] when the map is initialised already; [`ClusterError::partition_count`] when
    /// `partitions` is 0 or above 1,024; [`ClusterError::log`].
    pub fn init_placement(
        &mut self,
        partitions: u32,
        leader: NodeId,
    ) -> Result<Position, ClusterError> {
        self.append(MetadataCommand::InitPlacement { partitions, leader })
    }

    /// Commits the layout `assignments` as the next pending placement version, which takes effect at the epoch after
    /// the sequencer's close announcing it ([`MetadataState::pending_placement`](crate::MetadataState::pending_placement)
    /// names the oldest, which the host hands the next close); returns the position it was appended at.
    ///
    /// # Errors
    ///
    /// [`ClusterError::no_placement`] before the map is initialised; [`ClusterError::too_many`] past
    /// [`MetadataCommand::ASSIGNMENTS_MAX`](crate::MetadataCommand::ASSIGNMENTS_MAX) ranges; [`ClusterError::full`] at
    /// [`MetadataState::PLACEMENT_PENDING_MAX`](crate::MetadataState::PLACEMENT_PENDING_MAX) pending versions or with
    /// no room in the map for one more layout; the refusals of `PlacementMap::check_layout`, naming the first bucket
    /// left unassigned or assigned twice; [`ClusterError::log`]. Nothing changes on a refusal.
    pub fn change_placement(
        &mut self,
        assignments: &[(BucketRange, PartitionId)],
    ) -> Result<Position, ClusterError> {
        self.append(MetadataCommand::ChangePlacement {
            assignments: assignments.to_vec(),
        })
    }

    /// Makes `node` the preferred leader of `partition`; returns the position it was appended at.
    ///
    /// # Errors
    ///
    /// [`ClusterError::no_placement`] before the map is initialised; [`ClusterError::unknown_partition`] when its
    /// latest layout assigns `partition` no bucket; [`ClusterError::log`].
    pub fn set_preferred_leader(
        &mut self,
        partition: PartitionId,
        node: NodeId,
    ) -> Result<Position, ClusterError> {
        self.append(MetadataCommand::SetLeader { partition, node })
    }
}
