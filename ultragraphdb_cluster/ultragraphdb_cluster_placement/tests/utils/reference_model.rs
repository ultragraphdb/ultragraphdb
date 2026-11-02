/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A plain reference model of a layout, independent of the map's code: assignments as a list, each bucket answered by
//! a linear scan over every assignment, with no sort, no merge and no binary search.

use ultragraphdb_cluster_placement::{BucketRange, PartitionId};
use ultragraphdb_error_cluster::ClusterErrorEnum;

/// The bucket after the last.
pub const END: u32 = 1 << 24;

/// How many assignments cover `bucket`.
pub fn coverage(assignments: &[(BucketRange, PartitionId)], bucket: u32) -> usize {
    assignments
        .iter()
        .filter(|(range, _)| range.start() <= bucket && bucket < range.end())
        .count()
}

/// The partition of the one assignment covering `bucket`; `None` when none or several do.
pub fn owner(assignments: &[(BucketRange, PartitionId)], bucket: u32) -> Option<PartitionId> {
    let mut covering = assignments
        .iter()
        .filter(|(range, _)| range.start() <= bucket && bucket < range.end());
    let first = covering.next()?;
    match covering.next() {
        Some(_) => None,
        None => Some(first.1),
    }
}

/// The refusal a change with `assignments` must meet, naming its first bucket covered by no assignment or by several;
/// `None` for a layout covering every bucket once. A bucket's coverage changes only at a range's start or end, so the
/// first faulty bucket is bucket 0, a start or an end: each such bucket is counted bucket by bucket.
pub fn first_fault(assignments: &[(BucketRange, PartitionId)]) -> Option<ClusterErrorEnum> {
    let mut candidates: Vec<u32> = std::iter::once(0)
        .chain(assignments.iter().flat_map(|(r, _)| [r.start(), r.end()]))
        .filter(|&bucket| bucket < END)
        .collect();
    candidates.sort_unstable();
    candidates.dedup();
    candidates
        .into_iter()
        .find_map(|bucket| match coverage(assignments, bucket) {
            0 => Some(ClusterErrorEnum::Unassigned { bucket }),
            1 => None,
            _ => Some(ClusterErrorEnum::AssignedTwice { bucket }),
        })
}

/// A seeded xorshift64 generator, so a failing layout replays.
pub struct Rng(u64);

impl Rng {
    /// The generator seeded with `seed`, which must not be 0.
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next 64 bits.
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// A number below `bound`, which must not be 0.
    pub fn below(&mut self, bound: u32) -> u32 {
        u32::try_from(self.next() % u64::from(bound)).unwrap_or_default()
    }
}
