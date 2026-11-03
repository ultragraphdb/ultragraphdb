/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod placement_body_builder;

use crate::types::bucket_range::BucketRange;
use crate::types::namespace_run::NamespaceRun;
use crate::types::preferred_leader::PreferredLeader;

/// The placement map in effect at an epoch (`PROTOCOL.md` §13.6): every bucket's partition, each namespace's run and
/// each partition's preferred leader, with the epochs between which the map applies. A body with `from` 0 is a request.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `position` | u64: the metadata group's log position the map was read at; 0 in a request |
/// | 8 | `epoch` | epoch: the epoch the map is asked for; 0 for the latest |
/// | 16 | `from` | epoch: the epoch from which the map applies, at least 1; 0 in a request |
/// | 24 | `until` | epoch: the epoch from which the next change applies; 0 for none, and in a request |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | bucket ranges, 16 bytes each ([`BucketRange`]), covering every bucket once, in order |
/// | 1 | namespace runs, 16 bytes each ([`NamespaceRun`]), by namespace |
/// | 2 | preferred leaders, 8 bytes each ([`PreferredLeader`]), by partition |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    position: u64,
    epoch: u64,
    from: u64,
    until: u64,
    ranges: Vec<BucketRange>,
    runs: Vec<NamespaceRun>,
    leaders: Vec<PreferredLeader>,
}

impl Placement {
    /// A request for the map in effect at `epoch`, 0 for the latest.
    pub fn request(epoch: u64) -> Self {
        Self::reply(0, epoch, 0, 0)
    }

    /// A reply naming the map read at `position` for `epoch`, applying from `from` until `until`, with no range, run
    /// or leader yet.
    pub fn reply(position: u64, epoch: u64, from: u64, until: u64) -> Self {
        Self {
            position,
            epoch,
            from,
            until,
            ranges: Vec::new(),
            runs: Vec::new(),
            leaders: Vec::new(),
        }
    }

    /// `range` appended.
    pub fn push_range(&mut self, range: BucketRange) {
        self.ranges.push(range);
    }

    /// `run` appended.
    pub fn push_run(&mut self, run: NamespaceRun) {
        self.runs.push(run);
    }

    /// `leader` appended.
    pub fn push_leader(&mut self, leader: PreferredLeader) {
        self.leaders.push(leader);
    }

    /// The metadata group's log position; 0 in a request.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// The epoch the map is asked for; 0 for the latest.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The epoch from which the map applies; 0 in a request.
    pub fn from(&self) -> u64 {
        self.from
    }

    /// The epoch from which the next change applies; 0 for none.
    pub fn until(&self) -> u64 {
        self.until
    }

    /// The bucket ranges.
    pub fn ranges(&self) -> &[BucketRange] {
        &self.ranges
    }

    /// The namespace runs.
    pub fn runs(&self) -> &[NamespaceRun] {
        &self.runs
    }

    /// The preferred leaders.
    pub fn leaders(&self) -> &[PreferredLeader] {
        &self.leaders
    }
}
