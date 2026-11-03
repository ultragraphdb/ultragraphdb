/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// Where a part of a split list starts, appended to its split key after a kind byte: kind 1 and a literal's 64-bit
/// fingerprint as 8 big-endian bytes, or kind 2 and an ID as its 16 big-endian bytes. The kind byte keeps a key cut
/// inside an ID start from reading as one with a fingerprint start, and orders a list's split keys as their starts
/// order as 128-bit posting keys, a fingerprint below every ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SplitStart {
    /// A part of a list of IDs, starting at this ID.
    Id(SystemId),
    /// A part of a list of literals, starting at this fingerprint.
    Fingerprint(u64),
}
