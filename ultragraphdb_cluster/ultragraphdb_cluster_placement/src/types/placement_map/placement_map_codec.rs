/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

// Arithmetic on decoded store input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

use std::collections::BTreeMap;

use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;

use crate::types::bucket_range::BucketRange;
use crate::types::node_id::NodeId;
use crate::types::partition_id::PartitionId;
use crate::types::placement_map::PlacementMap;
use crate::types::placement_map::layout::Layout;
use crate::types::reader::Reader;

/// The format byte of the encoding.
const FORMAT: u8 = 1;

impl PlacementMap {
    /// Appends the map's encoding to `out`, big-endian throughout: the format byte 1; the layout count as 2 bytes;
    /// per layout, the epoch it applies from as 8 bytes, its range count as 4, and per range its first bucket and its
    /// partition as 4 each; the leader count as 4; per leader, in partition order, the partition and the node as 4
    /// each. A map has one encoding.
    pub fn encode(&self, out: &mut Vec<u8>) {
        // At most 256 layouts, 2^18 runs each and 1,024 leaders: every count fits its field.
        let layouts = std::iter::once(&self.first).chain(&self.later);
        out.push(FORMAT);
        out.extend_from_slice(&count_u16(layouts.clone().count()).to_be_bytes());
        for layout in layouts {
            out.extend_from_slice(&layout.from.get().to_be_bytes());
            out.extend_from_slice(&count_u32(layout.runs.len()).to_be_bytes());
            for (start, partition) in &layout.runs {
                out.extend_from_slice(&start.to_be_bytes());
                out.extend_from_slice(&partition.get().to_be_bytes());
            }
        }
        out.extend_from_slice(&count_u32(self.leaders.len()).to_be_bytes());
        for (partition, node) in &self.leaders {
            out.extend_from_slice(&partition.get().to_be_bytes());
            out.extend_from_slice(&node.get().to_be_bytes());
        }
    }

    /// The map `bytes` encode, exactly as [`PlacementMap::encode`] writes it.
    ///
    /// # Errors
    ///
    /// [`ClusterError::malformed`] naming the first value that is not as the encoding writes it, and its offset: a
    /// format byte other than 1; a count of 0 or past its limit; a first layout not from epoch 0, an epoch past 42
    /// bits or not after the layout before; a first range not at bucket 0, a range not after the one before or past
    /// the last bucket; partition 0; neighbouring ranges of one partition; a leader of a partition the latest layout
    /// does not hold, or out of partition order; a truncated value; bytes after the end.
    pub fn decode(bytes: &[u8]) -> Result<Self, ClusterError> {
        let mut reader = Reader::new(bytes);
        if reader.u8("the format byte 1")? != FORMAT {
            return Err(ClusterError::malformed("the format byte 1", 0));
        }
        let count_at = reader.offset();
        let count = reader.u16("a layout count from 1 to 256")?;
        if count == 0 || u32::from(count) > Self::LAYOUTS_MAX {
            return Err(ClusterError::malformed(
                "a layout count from 1 to 256",
                count_at,
            ));
        }
        let first = read_layout(&mut reader, None)?;
        // One per layout after the first.
        let mut later: Vec<Layout> = Vec::with_capacity((1..count).len());
        for _ in 1..count {
            let before = later.last().unwrap_or(&first).from;
            later.push(read_layout(&mut reader, Some(before))?);
        }
        let latest = later.last().unwrap_or(&first);
        let leaders = read_leaders(&mut reader, latest)?;
        reader.finish()?;
        Ok(Self {
            first,
            later,
            leaders,
        })
    }
}

/// `count`, at most a layout count's limit, as its 2-byte field.
fn count_u16(count: usize) -> u16 {
    debug_assert!(count <= PlacementMap::LAYOUTS_MAX as usize); // 256, lossless
    u16::try_from(count).unwrap_or(u16::MAX)
}

/// `count`, at most a range or leader count's limit, as its 4-byte field.
fn count_u32(count: usize) -> u32 {
    debug_assert!(count <= PlacementMap::RANGES_MAX as usize); // 262,144, lossless
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The partition at the reader, refused when 0.
fn read_partition(reader: &mut Reader<'_>) -> Result<PartitionId, ClusterError> {
    let at = reader.offset();
    let partition = reader.u32("a partition from 1")?;
    PartitionId::new(partition).map_err(|_| ClusterError::malformed("a partition from 1", at))
}

/// The next layout: the first, from epoch 0, when `before` is `None`, else one from an epoch after `before`.
fn read_layout(reader: &mut Reader<'_>, before: Option<Epoch>) -> Result<Layout, ClusterError> {
    let epoch_at = reader.offset();
    let raw = reader.u64("a layout's epoch")?;
    let from =
        Epoch::new(raw).map_err(|_| ClusterError::malformed("an epoch of 42 bits", epoch_at))?;
    match before {
        None if raw != 0 => {
            return Err(ClusterError::malformed(
                "the first layout from epoch 0",
                epoch_at,
            ));
        }
        Some(before) if from <= before => {
            return Err(ClusterError::malformed(
                "a layout after the one before",
                epoch_at,
            ));
        }
        None | Some(_) => {}
    }
    let count_at = reader.offset();
    let count = reader.u32("a range count from 1 to 262144")?;
    if count == 0 || count > PlacementMap::RANGES_MAX {
        return Err(ClusterError::malformed(
            "a range count from 1 to 262144",
            count_at,
        ));
    }
    let mut layout = Layout::with_capacity(from, count as usize); // at most `RANGES_MAX`, lossless
    for index in 0..count {
        read_run(reader, &mut layout, index)?;
    }
    Ok(layout)
}

/// Run `index` of `layout`, checked against the runs before it.
fn read_run(reader: &mut Reader<'_>, layout: &mut Layout, index: u32) -> Result<(), ClusterError> {
    let start_at = reader.offset();
    let start = reader.u32("a range's first bucket")?;
    let previous = layout.runs.last().copied();
    match previous {
        None if start != 0 => {
            return Err(ClusterError::malformed(
                "the first range at bucket 0",
                start_at,
            ));
        }
        Some((before, _)) if start <= before => {
            return Err(ClusterError::malformed(
                "a range after the one before",
                start_at,
            ));
        }
        None | Some(_) if start >= BucketRange::END => {
            return Err(ClusterError::malformed(
                "a range below bucket 16777216",
                start_at,
            ));
        }
        None | Some(_) => {}
    }
    let partition_at = reader.offset();
    let partition = read_partition(reader)?;
    if previous.is_some_and(|(_, before)| before == partition) {
        return Err(ClusterError::malformed(
            "a partition other than its neighbour's",
            partition_at,
        ));
    }
    debug_assert_eq!(layout.runs.len(), index as usize); // below `RANGES_MAX`, lossless
    layout.push_run(start, partition);
    // `PARTITIONS_MAX` is 1,024, so its cast is lossless.
    if layout.partitions.len() > PlacementMap::PARTITIONS_MAX as usize {
        return Err(ClusterError::malformed(
            "at most 1024 partitions in a layout",
            partition_at,
        ));
    }
    Ok(())
}

/// The leaders, each of a partition `latest` holds, in partition order.
fn read_leaders(
    reader: &mut Reader<'_>,
    latest: &Layout,
) -> Result<BTreeMap<PartitionId, NodeId>, ClusterError> {
    let count_at = reader.offset();
    let count = reader.u32("a leader count up to 1024")?;
    if count > PlacementMap::PARTITIONS_MAX {
        return Err(ClusterError::malformed(
            "a leader count up to 1024",
            count_at,
        ));
    }
    let mut leaders = BTreeMap::new();
    let mut previous: Option<PartitionId> = None;
    for _ in 0..count {
        let leader_at = reader.offset();
        let partition = read_partition(reader)?;
        if previous.is_some_and(|before| before >= partition) {
            return Err(ClusterError::malformed(
                "leaders in partition order",
                leader_at,
            ));
        }
        if !latest.partitions.contains(&partition) {
            return Err(ClusterError::malformed(
                "a leader of a partition the latest layout holds",
                leader_at,
            ));
        }
        let node = NodeId::new(reader.u32("a leader's node")?);
        let _replaced: Option<NodeId> = leaders.insert(partition, node);
        previous = Some(partition);
    }
    Ok(leaders)
}
