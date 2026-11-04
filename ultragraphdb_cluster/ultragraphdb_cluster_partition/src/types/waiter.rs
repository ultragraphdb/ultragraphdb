/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::SystemId;

/// A registration the home answers once an entry is durable: the ticket it was accepted under, its entity's ID and the
/// member's generation it answers, `None` for one naming no bucket group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Waiter {
    pub ticket: u64,
    pub entity: SystemId,
    pub generation: Option<NonZeroU64>,
}
