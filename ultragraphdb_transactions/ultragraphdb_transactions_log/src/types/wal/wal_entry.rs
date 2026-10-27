/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;

/// Where a round of the log lies: the in-memory index the log reads and trims by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WalEntry {
    pub position: Position,
    pub parent: u64,
    pub checksum: u64,
    pub segment: u64,
    pub offset: u64,
    pub size: u32,
}
