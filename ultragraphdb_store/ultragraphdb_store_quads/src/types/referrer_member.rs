/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, Epoch};

/// One member of an entity's referrer set (partitions, "An entity's referrer set names where its inbound edges may
/// be"): a subject bucket group that may hold edges to the entity, the epoch the member was added at, kept when it is
/// renewed, and its generation, at least 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferrerMember {
    group: BucketGroup,
    epoch: Epoch,
    generation: NonZeroU64,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<ReferrerMember>() <= 64);

impl ReferrerMember {
    /// The member for `group`, added at `epoch`, at `generation`.
    #[must_use]
    pub fn new(group: BucketGroup, epoch: Epoch, generation: NonZeroU64) -> Self {
        Self {
            group,
            epoch,
            generation,
        }
    }

    /// The bucket group.
    #[must_use]
    pub fn group(&self) -> BucketGroup {
        self.group
    }

    /// The epoch the member was added at.
    #[must_use]
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// The member's generation.
    #[must_use]
    pub fn generation(&self) -> NonZeroU64 {
        self.generation
    }
}
