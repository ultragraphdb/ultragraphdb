/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::cluster_error::{ClusterError, ClusterErrorEnum};

impl fmt::Display for ClusterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ClusterErrorEnum::ZeroPartition
            | ClusterErrorEnum::PartitionCount { .. }
            | ClusterErrorEnum::BucketRange { .. }
            | ClusterErrorEnum::Unassigned { .. }
            | ClusterErrorEnum::AssignedTwice { .. }
            | ClusterErrorEnum::ChangeEpoch { .. }
            | ClusterErrorEnum::UnknownPartition { .. } => fmt_placement(&self.0, f),
            ClusterErrorEnum::SettingBelow { .. } | ClusterErrorEnum::SettingAbove { .. } => {
                fmt_settings(&self.0, f)
            }
            ClusterErrorEnum::EpochLimit { .. }
            | ClusterErrorEnum::EpochAhead { .. }
            | ClusterErrorEnum::BeforeFirstClose { .. }
            | ClusterErrorEnum::NoPartitions
            | ClusterErrorEnum::DuplicatePartition { .. }
            | ClusterErrorEnum::NotSubscribed { .. }
            | ClusterErrorEnum::CatchUpTrimmed { .. } => fmt_sequencer(&self.0, f),
            ClusterErrorEnum::TooMany { .. }
            | ClusterErrorEnum::Full { .. }
            | ClusterErrorEnum::Malformed { .. }
            | ClusterErrorEnum::ReplayDiverged { .. }
            | ClusterErrorEnum::SnapshotBehind { .. }
            | ClusterErrorEnum::Log(_)
            | ClusterErrorEnum::Invariant(_) => fmt_damage(&self.0, f),
            ClusterErrorEnum::StaleCatalog { .. }
            | ClusterErrorEnum::NotPending { .. }
            | ClusterErrorEnum::CatalogReleased { .. }
            | ClusterErrorEnum::BlockSize { .. }
            | ClusterErrorEnum::SequencesSpent { .. }
            | ClusterErrorEnum::NoPlacement
            | ClusterErrorEnum::Contradicts { .. } => fmt_catalog(&self.0, f),
            ClusterErrorEnum::PrincipalName { .. }
            | ClusterErrorEnum::PrincipalExists { .. }
            | ClusterErrorEnum::UnknownPrincipal { .. }
            | ClusterErrorEnum::LastAdministrator { .. }
            | ClusterErrorEnum::Bootstrap { .. }
            | ClusterErrorEnum::SessionExists { .. }
            | ClusterErrorEnum::SessionEvicted { .. } => fmt_access(&self.0, f),
        }
    }
}

/// The texts of the placement map's refusals.
fn fmt_placement(error: &ClusterErrorEnum, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
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
        // fmt passes these to the other helpers and never here; their fields stand in for a text.
        ClusterErrorEnum::SettingBelow { .. }
        | ClusterErrorEnum::SettingAbove { .. }
        | ClusterErrorEnum::EpochLimit { .. }
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
        | ClusterErrorEnum::SnapshotBehind { .. }
        | ClusterErrorEnum::Log(_)
        | ClusterErrorEnum::Invariant(_)
        | ClusterErrorEnum::StaleCatalog { .. }
        | ClusterErrorEnum::NotPending { .. }
        | ClusterErrorEnum::CatalogReleased { .. }
        | ClusterErrorEnum::BlockSize { .. }
        | ClusterErrorEnum::SequencesSpent { .. }
        | ClusterErrorEnum::NoPlacement
        | ClusterErrorEnum::Contradicts { .. }
        | ClusterErrorEnum::PrincipalName { .. }
        | ClusterErrorEnum::PrincipalExists { .. }
        | ClusterErrorEnum::UnknownPrincipal { .. }
        | ClusterErrorEnum::LastAdministrator { .. }
        | ClusterErrorEnum::Bootstrap { .. }
        | ClusterErrorEnum::SessionExists { .. }
        | ClusterErrorEnum::SessionEvicted { .. } => write!(f, "{error:?}"),
    }
}

/// The texts of the settings' refusals.
fn fmt_settings(error: &ClusterErrorEnum, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
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
        // fmt passes these to the other helpers and never here; their fields stand in for a text.
        ClusterErrorEnum::ZeroPartition
        | ClusterErrorEnum::PartitionCount { .. }
        | ClusterErrorEnum::BucketRange { .. }
        | ClusterErrorEnum::Unassigned { .. }
        | ClusterErrorEnum::AssignedTwice { .. }
        | ClusterErrorEnum::ChangeEpoch { .. }
        | ClusterErrorEnum::UnknownPartition { .. }
        | ClusterErrorEnum::EpochLimit { .. }
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
        | ClusterErrorEnum::SnapshotBehind { .. }
        | ClusterErrorEnum::Log(_)
        | ClusterErrorEnum::Invariant(_)
        | ClusterErrorEnum::StaleCatalog { .. }
        | ClusterErrorEnum::NotPending { .. }
        | ClusterErrorEnum::CatalogReleased { .. }
        | ClusterErrorEnum::BlockSize { .. }
        | ClusterErrorEnum::SequencesSpent { .. }
        | ClusterErrorEnum::NoPlacement
        | ClusterErrorEnum::Contradicts { .. }
        | ClusterErrorEnum::PrincipalName { .. }
        | ClusterErrorEnum::PrincipalExists { .. }
        | ClusterErrorEnum::UnknownPrincipal { .. }
        | ClusterErrorEnum::LastAdministrator { .. }
        | ClusterErrorEnum::Bootstrap { .. }
        | ClusterErrorEnum::SessionExists { .. }
        | ClusterErrorEnum::SessionEvicted { .. } => write!(f, "{error:?}"),
    }
}

/// The texts of the sequencer's refusals.
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
        // fmt passes these to the other helpers and never here; their fields stand in for a text.
        ClusterErrorEnum::ZeroPartition
        | ClusterErrorEnum::PartitionCount { .. }
        | ClusterErrorEnum::BucketRange { .. }
        | ClusterErrorEnum::Unassigned { .. }
        | ClusterErrorEnum::AssignedTwice { .. }
        | ClusterErrorEnum::ChangeEpoch { .. }
        | ClusterErrorEnum::UnknownPartition { .. }
        | ClusterErrorEnum::SettingBelow { .. }
        | ClusterErrorEnum::SettingAbove { .. }
        | ClusterErrorEnum::TooMany { .. }
        | ClusterErrorEnum::Full { .. }
        | ClusterErrorEnum::Malformed { .. }
        | ClusterErrorEnum::ReplayDiverged { .. }
        | ClusterErrorEnum::SnapshotBehind { .. }
        | ClusterErrorEnum::Log(_)
        | ClusterErrorEnum::Invariant(_)
        | ClusterErrorEnum::StaleCatalog { .. }
        | ClusterErrorEnum::NotPending { .. }
        | ClusterErrorEnum::CatalogReleased { .. }
        | ClusterErrorEnum::BlockSize { .. }
        | ClusterErrorEnum::SequencesSpent { .. }
        | ClusterErrorEnum::NoPlacement
        | ClusterErrorEnum::Contradicts { .. }
        | ClusterErrorEnum::PrincipalName { .. }
        | ClusterErrorEnum::PrincipalExists { .. }
        | ClusterErrorEnum::UnknownPrincipal { .. }
        | ClusterErrorEnum::LastAdministrator { .. }
        | ClusterErrorEnum::Bootstrap { .. }
        | ClusterErrorEnum::SessionExists { .. }
        | ClusterErrorEnum::SessionEvicted { .. } => write!(f, "{error:?}"),
    }
}

/// The texts of the bounds, the damage and the carried errors.
fn fmt_damage(error: &ClusterErrorEnum, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
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
        ClusterErrorEnum::SnapshotBehind {
            snapshot,
            checkpoint,
        } => write!(
            f,
            "the snapshot at log position {snapshot} lies below the log's checkpoint {checkpoint}: the rounds between are trimmed"
        ),
        ClusterErrorEnum::Log(error) => write!(f, "ordered log: {error}"),
        ClusterErrorEnum::Invariant(error) => write!(f, "{error}"),
        // fmt passes these to the other helpers and never here; their fields stand in for a text.
        ClusterErrorEnum::ZeroPartition
        | ClusterErrorEnum::PartitionCount { .. }
        | ClusterErrorEnum::BucketRange { .. }
        | ClusterErrorEnum::Unassigned { .. }
        | ClusterErrorEnum::AssignedTwice { .. }
        | ClusterErrorEnum::ChangeEpoch { .. }
        | ClusterErrorEnum::UnknownPartition { .. }
        | ClusterErrorEnum::SettingBelow { .. }
        | ClusterErrorEnum::SettingAbove { .. }
        | ClusterErrorEnum::EpochLimit { .. }
        | ClusterErrorEnum::EpochAhead { .. }
        | ClusterErrorEnum::BeforeFirstClose { .. }
        | ClusterErrorEnum::NoPartitions
        | ClusterErrorEnum::DuplicatePartition { .. }
        | ClusterErrorEnum::NotSubscribed { .. }
        | ClusterErrorEnum::CatchUpTrimmed { .. }
        | ClusterErrorEnum::StaleCatalog { .. }
        | ClusterErrorEnum::NotPending { .. }
        | ClusterErrorEnum::CatalogReleased { .. }
        | ClusterErrorEnum::BlockSize { .. }
        | ClusterErrorEnum::SequencesSpent { .. }
        | ClusterErrorEnum::NoPlacement
        | ClusterErrorEnum::Contradicts { .. }
        | ClusterErrorEnum::PrincipalName { .. }
        | ClusterErrorEnum::PrincipalExists { .. }
        | ClusterErrorEnum::UnknownPrincipal { .. }
        | ClusterErrorEnum::LastAdministrator { .. }
        | ClusterErrorEnum::Bootstrap { .. }
        | ClusterErrorEnum::SessionExists { .. }
        | ClusterErrorEnum::SessionEvicted { .. } => write!(f, "{error:?}"),
    }
}

/// The texts of the metadata group's refusals of catalog versions, block leases, the placement map and recorded decisions.
fn fmt_catalog(error: &ClusterErrorEnum, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
        ClusterErrorEnum::StaleCatalog {
            tenant,
            predecessor,
            current,
        } => write!(
            f,
            "a catalog version of tenant {tenant} built on version {predecessor}, while version {current} is its latest"
        ),
        ClusterErrorEnum::NotPending { tenant, version } => write!(
            f,
            "a close announces version {version} of tenant {tenant}'s catalog, which is not pending"
        ),
        ClusterErrorEnum::CatalogReleased { tenant, epoch } => write!(
            f,
            "the catalog version of tenant {tenant} in effect at epoch {epoch} is no longer retained"
        ),
        ClusterErrorEnum::BlockSize { size, max } => write!(
            f,
            "a block of {size} sequences; a lease takes from 1 to {max}, block_size_max"
        ),
        ClusterErrorEnum::SequencesSpent { prefix } => write!(
            f,
            "the sequences of type prefix {prefix} are spent: no block of the size asked for lies below 2^64"
        ),
        ClusterErrorEnum::NoPlacement => write!(
            f,
            "the metadata group holds no placement map: it is initialised at the cluster's first start"
        ),
        ClusterErrorEnum::Contradicts { what } => write!(
            f,
            "the command contradicts the state it is applied to: {what}"
        ),
        // fmt passes these to the other helpers and never here; their fields stand in for a text.
        ClusterErrorEnum::ZeroPartition
        | ClusterErrorEnum::PartitionCount { .. }
        | ClusterErrorEnum::BucketRange { .. }
        | ClusterErrorEnum::Unassigned { .. }
        | ClusterErrorEnum::AssignedTwice { .. }
        | ClusterErrorEnum::ChangeEpoch { .. }
        | ClusterErrorEnum::UnknownPartition { .. }
        | ClusterErrorEnum::SettingBelow { .. }
        | ClusterErrorEnum::SettingAbove { .. }
        | ClusterErrorEnum::EpochLimit { .. }
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
        | ClusterErrorEnum::SnapshotBehind { .. }
        | ClusterErrorEnum::Log(_)
        | ClusterErrorEnum::Invariant(_)
        | ClusterErrorEnum::PrincipalName { .. }
        | ClusterErrorEnum::PrincipalExists { .. }
        | ClusterErrorEnum::UnknownPrincipal { .. }
        | ClusterErrorEnum::LastAdministrator { .. }
        | ClusterErrorEnum::Bootstrap { .. }
        | ClusterErrorEnum::SessionExists { .. }
        | ClusterErrorEnum::SessionEvicted { .. } => write!(f, "{error:?}"),
    }
}

/// The texts of the metadata group's refusals of principals and sessions.
fn fmt_access(error: &ClusterErrorEnum, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
        ClusterErrorEnum::PrincipalName { length } => write!(
            f,
            "a principal's name of {length} bytes; a name holds from 1 to 255"
        ),
        ClusterErrorEnum::PrincipalExists { name } => {
            write!(f, "a principal named {name} exists")
        }
        ClusterErrorEnum::UnknownPrincipal { name } => {
            write!(f, "no principal is named {name}")
        }
        ClusterErrorEnum::LastAdministrator { name } => write!(
            f,
            "{name} is the last principal holding the administrator role, which the cluster keeps"
        ),
        ClusterErrorEnum::Bootstrap { what } => {
            write!(f, "the bootstrap principal is not created: it needs {what}")
        }
        ClusterErrorEnum::SessionExists { session } => {
            write!(f, "session {session:032x} is registered already")
        }
        ClusterErrorEnum::SessionEvicted { session } => write!(
            f,
            "session {session:032x} is not in the registry: it was evicted, or never registered"
        ),
        // fmt passes these to the other helpers and never here; their fields stand in for a text.
        ClusterErrorEnum::ZeroPartition
        | ClusterErrorEnum::PartitionCount { .. }
        | ClusterErrorEnum::BucketRange { .. }
        | ClusterErrorEnum::Unassigned { .. }
        | ClusterErrorEnum::AssignedTwice { .. }
        | ClusterErrorEnum::ChangeEpoch { .. }
        | ClusterErrorEnum::UnknownPartition { .. }
        | ClusterErrorEnum::SettingBelow { .. }
        | ClusterErrorEnum::SettingAbove { .. }
        | ClusterErrorEnum::EpochLimit { .. }
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
        | ClusterErrorEnum::SnapshotBehind { .. }
        | ClusterErrorEnum::Log(_)
        | ClusterErrorEnum::Invariant(_)
        | ClusterErrorEnum::StaleCatalog { .. }
        | ClusterErrorEnum::NotPending { .. }
        | ClusterErrorEnum::CatalogReleased { .. }
        | ClusterErrorEnum::BlockSize { .. }
        | ClusterErrorEnum::SequencesSpent { .. }
        | ClusterErrorEnum::NoPlacement
        | ClusterErrorEnum::Contradicts { .. } => write!(f, "{error:?}"),
    }
}
