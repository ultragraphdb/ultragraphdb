/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod invariant_display;

/// What the simulator checks after every step and every recovery. `Display` states the rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Invariant {
    /// A batch is visible whole or not at all.
    BatchWhole,
    /// Every synced batch survives a crash or a power loss.
    SyncedBatchSurvives,
    /// The batches that survive a power loss are a prefix of batch order.
    SurvivorsArePrefix,
    /// A read returns the newest version written at or below its timestamp, and nothing else.
    ReadsAtOrBelowTimestamp,
    /// A read made again through an open snapshot returns what it returned the first time, items and errors alike:
    /// a snapshot is a consistent view. The rule needs no model of garbage collection or corruption, so it also
    /// compares the corrupted keys the other read checks leave out. Two changes are allowed. An injected read
    /// failure (`ReadFailed`) is not the snapshot's answer, so a scan it cuts short is compared only up to the
    /// failure. A corruption injected since the first read, which an open snapshot sees as fjall's does, lets every
    /// read covering its key answer anew in each snapshot of the data at or above the corrupted version. A snapshot
    /// holds the answers of at most 64 reads at once; a read past them is not compared until a corruption frees a
    /// place.
    SnapshotReadsRepeat,
    /// A snapshot opens exactly when its timestamp is at or above the discard timestamp in effect, and the discard
    /// timestamp stays at or below every open snapshot.
    DiscardBelowOpenReads,
    /// A dropped prefix stays dropped.
    DroppedPrefixAbsent,
    /// No write lands below a deletion of the same key.
    NoWriteBelowDeletion,
    /// The applied timestamp is the highest commit timestamp the store holds; rollup and drop batches leave it
    /// unchanged.
    AppliedTsIsHighest,
    /// A failed batch is followed by recovery and nothing else.
    RecoveryAfterFailure,
    /// The store holds exactly what was written to it, less what drops and garbage collection removed.
    ContentsMatchModel,
    /// A rule a workload above the store adds, named by it.
    Workload(&'static str),
}
