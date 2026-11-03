/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::TypePrefix;

use crate::types::sequence_block::SequenceBlock;

/// The source a partition leases its sequence blocks from, per type: the metadata group in a cluster.
///
/// Every block a source hands out, to any partition, is a half-open range of at least one sequence that overlaps no
/// other block of the same type and is never handed out again, whether or not its partition used it: a partition
/// that restarts abandons the blocks it held and leases new ones. A partition leases its next block before its
/// current one runs out, and allocates no sequence outside a block it holds.
pub trait BlockSource {
    /// The next block of `prefix`'s sequences for this partition, or `None` when none can be leased now: the source
    /// is unreachable, or the type's sequences are spent. It returns at once; a source that fetches over the network
    /// hands out blocks it already holds.
    fn lease(&self, prefix: TypePrefix) -> Option<SequenceBlock>;
}
