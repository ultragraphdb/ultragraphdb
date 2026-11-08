/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{Epoch, SystemId};

/// A home's answer to a registration, once it is durable: the entity's ID, the member's generation, `None` for a
/// registration naming no bucket group, and the latest close delivered to the home, `c`, below the epoch it stamped the
/// registration at; the registrant commits a write referring to the entity only once close `c` is delivered to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    entity: SystemId,
    generation: Option<NonZeroU64>,
    close: Epoch,
}

impl Answer {
    /// The answer naming `entity`, `generation` and `close`.
    #[must_use]
    pub fn new(entity: SystemId, generation: Option<NonZeroU64>, close: Epoch) -> Self {
        Self {
            entity,
            generation,
            close,
        }
    }

    /// The entity's ID.
    #[must_use]
    pub fn entity(&self) -> SystemId {
        self.entity
    }

    /// The member's generation; `None` for a registration naming no bucket group.
    #[must_use]
    pub fn generation(&self) -> Option<NonZeroU64> {
        self.generation
    }

    /// The latest close delivered to the home.
    #[must_use]
    pub fn close(&self) -> Epoch {
        self.close
    }
}
