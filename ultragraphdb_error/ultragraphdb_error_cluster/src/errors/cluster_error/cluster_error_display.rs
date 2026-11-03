/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::cluster_error::{ClusterError, ClusterErrorEnum};

impl fmt::Display for ClusterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ClusterErrorEnum::ZeroPartition => {
                write!(
                    f,
                    "partition 0 does not exist; partitions are numbered from 1"
                )
            }
            ClusterErrorEnum::PartitionCount { count, max } => write!(
                f,
                "a placement map over {count} partitions; it takes from 1 to {max}"
            ),
            ClusterErrorEnum::BucketRange { start, end } => write!(
                f,
                "the bucket range {start}..{end} is empty or reaches past bucket 16777215"
            ),
            ClusterErrorEnum::Unassigned { bucket } => write!(
                f,
                "the change leaves bucket {bucket} assigned to no partition"
            ),
            ClusterErrorEnum::AssignedTwice { bucket } => write!(
                f,
                "the change assigns bucket {bucket} to more than one partition"
            ),
            ClusterErrorEnum::ChangeEpoch { epoch, latest } => write!(
                f,
                "a change from epoch {epoch} does not follow the latest change, from epoch {latest}"
            ),
            ClusterErrorEnum::UnknownPartition { partition } => write!(
                f,
                "partition {partition} holds no bucket in the latest layout"
            ),
            ClusterErrorEnum::SettingBelow {
                setting,
                value,
                bound,
                minimum,
            } => write!(f, "{setting} is {value}, below {bound}, {minimum}"),
            ClusterErrorEnum::SettingAbove {
                setting,
                value,
                maximum,
            } => write!(f, "{setting} is {value}, above its maximum, {maximum}"),
            ClusterErrorEnum::EpochLimit { .. }
            | ClusterErrorEnum::EpochAhead { .. }
            | ClusterErrorEnum::BeforeFirstClose { .. }
            | ClusterErrorEnum::NoPartitions
            | ClusterErrorEnum::DuplicatePartition { .. }
            | ClusterErrorEnum::NotSubscribed { .. }
            | ClusterErrorEnum::CatchUpTrimmed { .. }
            | ClusterErrorEnum::TooMany { .. }
            | ClusterErrorEnum::Full { .. }
            | ClusterErrorEnum::Malformed { .. }
            | ClusterErrorEnum::ReplayDiverged { .. }
            | ClusterErrorEnum::Log(_)
            | ClusterErrorEnum::Invariant(_) => fmt_sequencer(&self.0, f),
        }
    }
}

/// The texts of the sequencer's refusals, the bounds, the damage and the carried errors.
fn fmt_sequencer(error: &ClusterErrorEnum, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
        ClusterErrorEnum::EpochLimit { epoch } => write!(
            f,
            "no epoch follows epoch {epoch}, the largest a 42-bit epoch number holds"
        ),
        ClusterErrorEnum::EpochAhead { epoch, latest } => write!(
            f,
            "epoch {epoch} is ahead of epoch {latest}, the furthest the sequencer has reached for it"
        ),
        ClusterErrorEnum::BeforeFirstClose {
            time_us,
            first_us: Some(first_us),
        } => write!(
            f,
            "the time {time_us} µs lies before the first close, at {first_us} µs"
        ),
        ClusterErrorEnum::BeforeFirstClose {
            time_us,
            first_us: None,
        } => write!(
            f,
            "the time {time_us} µs lies before the first close; no epoch has closed"
        ),
        ClusterErrorEnum::NoPartitions => write!(
            f,
            "a multi-partition transaction names no partition: its partition list is empty"
        ),
        ClusterErrorEnum::DuplicatePartition { partition } => write!(
            f,
            "a multi-partition transaction names partition {partition} twice"
        ),
        ClusterErrorEnum::NotSubscribed { partition } => write!(
            f,
            "partition {partition} is not subscribed to the sequencer's closes"
        ),
        ClusterErrorEnum::CatchUpTrimmed { after, oldest } => write!(
            f,
            "the closes after epoch {after} are no longer held; the oldest held is epoch {oldest}"
        ),
        ClusterErrorEnum::TooMany { what, count, max } => {
            write!(f, "{what}: {count}, more than the limit {max}")
        }
        ClusterErrorEnum::Full { what, capacity } => {
            write!(f, "{what} holds {capacity} entries, its capacity")
        }
        ClusterErrorEnum::Malformed { what, offset } => {
            write!(f, "malformed encoding: {what} at byte {offset}")
        }
        ClusterErrorEnum::ReplayDiverged { position, what } => write!(
            f,
            "the round at log position {position} contradicts the log before it: {what}"
        ),
        ClusterErrorEnum::Log(error) => write!(f, "ordered log: {error}"),
        ClusterErrorEnum::Invariant(error) => write!(f, "{error}"),
        ClusterErrorEnum::ZeroPartition
        | ClusterErrorEnum::PartitionCount { .. }
        | ClusterErrorEnum::BucketRange { .. }
        | ClusterErrorEnum::Unassigned { .. }
        | ClusterErrorEnum::AssignedTwice { .. }
        | ClusterErrorEnum::ChangeEpoch { .. }
        | ClusterErrorEnum::UnknownPartition { .. }
        | ClusterErrorEnum::SettingBelow { .. }
        // fmt writes these itself and never passes them here; their fields stand in for a text.
        | ClusterErrorEnum::SettingAbove { .. } => write!(f, "{error:?}"),
    }
}
