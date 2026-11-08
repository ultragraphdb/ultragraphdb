/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod placement_view_body_view;

use crate::types::bucket_range::BucketRange;
use crate::types::preferred_leader::PreferredLeader;
use crate::types::tenant_run::TenantRun;
use crate::utils::whole_records::{record_at, record_count};

/// The body of a received Placement frame, read in place; see [`crate::Placement`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementView<'a> {
    position: u64,
    epoch: u64,
    from: u64,
    until: u64,
    ranges: &'a [u8],
    runs: &'a [u8],
    leaders: &'a [u8],
}

impl<'a> PlacementView<'a> {
    /// The metadata group's log position; 0 in a request.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.position
    }

    /// The epoch the map is asked for; 0 for the latest.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The epoch from which the map applies; 0 in a request.
    #[must_use]
    pub fn from(&self) -> u64 {
        self.from
    }

    /// The epoch from which the next change applies; 0 for none.
    #[must_use]
    pub fn until(&self) -> u64 {
        self.until
    }

    /// Whether the body is a request: `from` 0.
    #[must_use]
    pub fn is_request(&self) -> bool {
        self.from == 0
    }

    /// The number of bucket ranges.
    #[must_use]
    pub fn range_count(&self) -> usize {
        record_count::<{ BucketRange::SIZE }>(self.ranges)
    }

    /// Range `index`; `None` past the last.
    #[must_use]
    pub fn range(&self, index: usize) -> Option<BucketRange> {
        let bytes = record_at::<{ BucketRange::SIZE }>(self.ranges, index)?;
        BucketRange::decode(0, 0, bytes).ok()
    }

    /// The number of tenant runs.
    #[must_use]
    pub fn run_count(&self) -> usize {
        record_count::<{ TenantRun::SIZE }>(self.runs)
    }

    /// Run `index`; `None` past the last.
    pub fn run(&self, index: usize) -> Option<TenantRun> {
        let bytes = record_at::<{ TenantRun::SIZE }>(self.runs, index)?;
        TenantRun::decode(0, 0, bytes).ok()
    }

    /// The number of preferred leaders.
    #[must_use]
    pub fn leader_count(&self) -> usize {
        record_count::<{ PreferredLeader::SIZE }>(self.leaders)
    }

    /// Leader `index`; `None` past the last.
    #[must_use]
    pub fn leader(&self, index: usize) -> Option<PreferredLeader> {
        record_at::<{ PreferredLeader::SIZE }>(self.leaders, index)
            .map(|bytes| PreferredLeader::decode(*bytes))
    }
}
