/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// Where a part of a split list starts, appended to its split key: an ID as its 16 big-endian bytes for a list of IDs,
/// a literal's 64-bit fingerprint as 8 big-endian bytes for a list of literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SplitStart {
    /// A part of a list of IDs, starting at this ID.
    Id(SystemId),
    /// A part of a list of literals, starting at this fingerprint.
    Fingerprint(u64),
}
