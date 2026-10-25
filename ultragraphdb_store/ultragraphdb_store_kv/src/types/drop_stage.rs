/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A stage a drop on the fjall store has reached, reported in order by [`crate::FjallStore::drop_prefixes_observed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DropStage {
    /// The drop record written and synced; nothing removed yet.
    RecordSynced,
    /// A batch of removals committed, this many stored versions removed so far; more may follow.
    Removing(usize),
    /// Every stored version under the prefixes removed; the entries not yet written.
    DataRemoved,
    /// The entries written and synced; the record not yet deleted.
    EntriesSynced,
}
