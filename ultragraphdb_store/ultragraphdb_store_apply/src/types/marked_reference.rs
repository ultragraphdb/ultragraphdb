/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, SystemId};

/// A registration a transaction relied on as a referrer: the entity another partition owns, the subject bucket group
/// it referenced the entity from, and the generation the home's reply named. Its commit writes the mark of
/// `(entity, group)` at that generation, and its record lists it (write-set record section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkedReference {
    entity: SystemId,
    group: BucketGroup,
    generation: NonZeroU64,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<MarkedReference>() <= 64);

impl MarkedReference {
    /// The reference to `entity` from `group` at `generation`.
    #[must_use]
    pub fn new(entity: SystemId, group: BucketGroup, generation: NonZeroU64) -> Self {
        Self {
            entity,
            group,
            generation,
        }
    }

    /// The entity.
    #[must_use]
    pub fn entity(&self) -> SystemId {
        self.entity
    }

    /// The subject bucket group.
    #[must_use]
    pub fn group(&self) -> BucketGroup {
        self.group
    }

    /// The generation the home's reply named.
    #[must_use]
    pub fn generation(&self) -> NonZeroU64 {
        self.generation
    }
}
