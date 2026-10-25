/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::invariant::Invariant;

impl fmt::Display for Invariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rule = match self {
            Invariant::BatchWhole => "a batch is visible whole or not at all",
            Invariant::SyncedBatchSurvives => "every synced batch survives a crash or a power loss",
            Invariant::SurvivorsArePrefix => {
                "the batches that survive a power loss are a prefix of batch order"
            }
            Invariant::ReadsAtOrBelowTimestamp => {
                "a read returns the newest version written at or below its timestamp"
            }
            Invariant::SnapshotReadsRepeat => {
                "a read repeated through an open snapshot returns what it returned the first time"
            }
            Invariant::DiscardBelowOpenReads => {
                "a snapshot opens exactly at or above the discard timestamp, which stays at or below every open snapshot"
            }
            Invariant::DroppedPrefixAbsent => "a dropped prefix stays dropped",
            Invariant::NoWriteBelowDeletion => "no write lands below a deletion of the same key",
            Invariant::AppliedTsIsHighest => {
                "the applied timestamp is the highest commit timestamp the store holds"
            }
            Invariant::RecoveryAfterFailure => {
                "a failed batch is followed by recovery and nothing else"
            }
            Invariant::ContentsMatchModel => "the store holds exactly what was written to it",
            Invariant::Workload(rule) => rule,
        };
        f.write_str(rule)
    }
}
