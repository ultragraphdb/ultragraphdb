/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a request naming a tenant reads of the core's view once the gate passed it (design D6): the open and the latest
/// closed epoch, and the tenant's current catalog version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewFacts {
    pub open_epoch: u64,
    pub last_closed: u64,
    pub current_version: u64,
}
