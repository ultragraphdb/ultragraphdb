/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, Tenant};
use ultragraphdb_core_mapping::RandomSource;

/// The buckets of a tenant's run: its 2^16 buckets share their top 8 bits.
const RUN_BUCKETS: u32 = 1 << 16;

/// The buckets a partition owns, as sorted, disjoint ranges: where it draws a new blank node's bucket, among those of
/// the tenant's run. A store on its own owns every bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedBuckets {
    /// Half-open ranges of bucket numbers, sorted and disjoint.
    ranges: Vec<(u32, u32)>,
}

impl OwnedBuckets {
    /// Every bucket.
    #[must_use]
    pub fn all() -> Self {
        Self {
            ranges: vec![(0, Bucket::MAX.get() + 1)],
        }
    }

    /// The buckets of `ranges`, each its first and last bucket, or `None` unless every range's first bucket is at
    /// most its last and each range starts after the one before it ends.
    #[must_use]
    pub fn new(ranges: &[(Bucket, Bucket)]) -> Option<Self> {
        let mut owned = Vec::with_capacity(ranges.len());
        let mut after = 0u32;
        for (first, last) in ranges {
            if first > last || (!owned.is_empty() && first.get() < after) {
                return None;
            }
            // No overflow: a bucket is below 2^24.
            after = last.get() + 1;
            owned.push((first.get(), after));
        }
        Some(Self { ranges: owned })
    }

    /// Whether `bucket` is owned.
    #[must_use]
    pub fn owns(&self, bucket: Bucket) -> bool {
        self.ranges
            .iter()
            .any(|&(start, end)| (start..end).contains(&bucket.get()))
    }

    /// A bucket drawn uniformly from `random` among those owned in `tenant`'s run, the 2^16 buckets whose top 8
    /// bits are the tenant's hash; `None` when none of them is owned.
    pub fn draw(&self, tenant: Tenant, random: &mut RandomSource) -> Option<Bucket> {
        let run_start = u32::from(tenant.bucket_bits()) << 16;
        let in_run: Vec<(u32, u32)> = self
            .ranges
            .iter()
            .map(|&(start, end)| (start.max(run_start), end.min(run_start + RUN_BUCKETS)))
            .filter(|(start, end)| start < end)
            .collect();
        let owned: u32 = in_run.iter().map(|(start, end)| end - start).sum();
        debug_assert!(owned <= RUN_BUCKETS, "a run holds 2^16 buckets");
        if owned == 0 {
            return None;
        }
        let mut index = u32::try_from(random.below(u64::from(owned))).ok()?;
        for (start, end) in in_run {
            if index < end - start {
                return Bucket::new(start + index).ok();
            }
            index -= end - start;
        }
        None
    }
}
