/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How far a rebuild had come, as the store sees it, when a crash or a power loss stopped it. A rebuild writes its
/// record and syncs it, writes what it builds to a temporary keyspace, and removes the record with its swap batch or
/// its failure batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RebuildStage {
    /// The record not written yet.
    Start,
    /// The record handed to the operating system, not yet synced.
    RecordWritten,
    /// The record synced; nothing built yet.
    RecordSynced,
    /// A batch written to the temporary keyspace.
    TemporaryWritten,
    /// The batch removing the record handed to the operating system, not yet synced.
    RecordRemoved,
}

impl RebuildStage {
    /// The stages a rebuild passes, in order.
    pub const ALL: [RebuildStage; 5] = [
        RebuildStage::Start,
        RebuildStage::RecordWritten,
        RebuildStage::RecordSynced,
        RebuildStage::TemporaryWritten,
        RebuildStage::RecordRemoved,
    ];
}
