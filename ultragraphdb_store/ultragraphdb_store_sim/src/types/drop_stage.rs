/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How far a drop had come when a crash or a power loss stopped it. A drop writes its record, syncs it, removes
/// the data, writes and syncs its entries, and then deletes the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DropStage {
    /// Nothing written yet.
    Start,
    /// The record handed to the operating system, not yet synced.
    RecordWritten,
    /// The record synced; no data removed yet.
    RecordSynced,
    /// The data removed; the entries not yet written.
    DataRemoved,
    /// The entries written and synced; the record not yet deleted.
    EntriesSynced,
}

impl DropStage {
    /// The stages a drop passes, in order.
    pub const ALL: [DropStage; 5] = [
        DropStage::Start,
        DropStage::RecordWritten,
        DropStage::RecordSynced,
        DropStage::DataRemoved,
        DropStage::EntriesSynced,
    ];
}
