/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::damage::Damage;

/// Where the recovery's scan of the segments stopped: the block after the last round, and the damage found there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WalStop {
    pub segment: u64,
    pub offset: u64,
    pub damage: Option<Damage>,
}
