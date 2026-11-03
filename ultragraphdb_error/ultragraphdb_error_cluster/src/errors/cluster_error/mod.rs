/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod cluster_error_display;
mod cluster_error_error;
mod cluster_error_from;

use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_error_log::LogError;

/// Why an operation of the cluster's state machines was refused, grouped by what the caller does about it.
///
/// - **Placement:** [`ZeroPartition`](ClusterErrorEnum::ZeroPartition),
///   [`PartitionCount`](ClusterErrorEnum::PartitionCount), [`BucketRange`](ClusterErrorEnum::BucketRange),
///   [`Unassigned`](ClusterErrorEnum::Unassigned), [`AssignedTwice`](ClusterErrorEnum::AssignedTwice),
///   [`ChangeEpoch`](ClusterErrorEnum::ChangeEpoch),
///   [`UnknownPartition`](ClusterErrorEnum::UnknownPartition). A value or a change the placement map does not take;
///   the map is unchanged.
/// - **Configuration:** [`SettingBelow`](ClusterErrorEnum::SettingBelow),
///   [`SettingAbove`](ClusterErrorEnum::SettingAbove). A setting outside its bounds; the role does not start.
/// - **Sequencer:** [`EpochLimit`](ClusterErrorEnum::EpochLimit), [`EpochAhead`](ClusterErrorEnum::EpochAhead),
///   [`BeforeFirstClose`](ClusterErrorEnum::BeforeFirstClose), [`NoPartitions`](ClusterErrorEnum::NoPartitions),
///   [`DuplicatePartition`](ClusterErrorEnum::DuplicatePartition),
///   [`NotSubscribed`](ClusterErrorEnum::NotSubscribed), [`CatchUpTrimmed`](ClusterErrorEnum::CatchUpTrimmed). A
///   request the sequencer refuses; nothing is appended.
/// - **Metadata group:** [`StaleCatalog`](ClusterErrorEnum::StaleCatalog),
///   [`NotPending`](ClusterErrorEnum::NotPending), [`CatalogReleased`](ClusterErrorEnum::CatalogReleased),
///   [`BlockSize`](ClusterErrorEnum::BlockSize), [`SequencesSpent`](ClusterErrorEnum::SequencesSpent),
///   [`NoPlacement`](ClusterErrorEnum::NoPlacement), [`Contradicts`](ClusterErrorEnum::Contradicts),
///   [`PrincipalName`](ClusterErrorEnum::PrincipalName), [`PrincipalExists`](ClusterErrorEnum::PrincipalExists),
///   [`UnknownPrincipal`](ClusterErrorEnum::UnknownPrincipal),
///   [`LastAdministrator`](ClusterErrorEnum::LastAdministrator), [`Bootstrap`](ClusterErrorEnum::Bootstrap),
///   [`SessionExists`](ClusterErrorEnum::SessionExists), [`SessionEvicted`](ClusterErrorEnum::SessionEvicted). A
///   command or a read the metadata group refuses; nothing is appended.
/// - **Bounds:** [`TooMany`](ClusterErrorEnum::TooMany), [`Full`](ClusterErrorEnum::Full). A value past a limit, or
///   a table at its capacity; nothing changes.
/// - **Damage:** [`Malformed`](ClusterErrorEnum::Malformed), [`ReplayDiverged`](ClusterErrorEnum::ReplayDiverged).
///   Bytes or a log that do not hold what was written, and [`SnapshotBehind`](ClusterErrorEnum::SnapshotBehind), a
///   snapshot older than the log's checkpoint; the state machine does not start from them.
/// - **Carried:** [`Log`](ClusterErrorEnum::Log), the ordered log's own refusal; [`Invariant`](ClusterErrorEnum::Invariant),
///   a bug, on which the component stops.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClusterError(pub ClusterErrorEnum);

/// The classification of a [`ClusterError`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClusterErrorEnum {
    /// A partition ID of 0, which no partition has: partitions are numbered from 1.
    ZeroPartition,
    /// A map split evenly over `count` partitions, which is 0 or more than `max`.
    PartitionCount {
        /// The partitions asked for.
        count: u32,
        /// The most partitions a map holds.
        max: u32,
    },
    /// The bucket range `start..end` is empty or reaches past the last bucket, 2^24 − 1.
    BucketRange {
        /// The first bucket of the range.
        start: u32,
        /// The bucket after the range's last.
        end: u32,
    },
    /// A change leaves `bucket`, the first such bucket, assigned to no partition.
    Unassigned {
        /// The first bucket no range of the change covers.
        bucket: u32,
    },
    /// A change assigns `bucket`, the first such bucket, to more than one partition.
    AssignedTwice {
        /// The first bucket two ranges of the change cover.
        bucket: u32,
    },
    /// A change applying from `epoch` does not follow the latest change, which applies from `latest`.
    ChangeEpoch {
        /// The epoch the change names.
        epoch: u64,
        /// The epoch the latest change applies from.
        latest: u64,
    },
    /// `partition` holds no bucket in the latest layout of the map.
    UnknownPartition {
        /// The partition named.
        partition: u32,
    },
    /// The setting `setting` is `value`, below `bound`, which is `minimum`.
    SettingBelow {
        /// The setting's name.
        setting: &'static str,
        /// The value given.
        value: u64,
        /// What bounds it from below: its fixed minimum or another setting's name.
        bound: &'static str,
        /// The least value it takes.
        minimum: u64,
    },
    /// The setting `setting` is `value`, above its maximum `maximum`.
    SettingAbove {
        /// The setting's name.
        setting: &'static str,
        /// The value given.
        value: u64,
        /// The largest value it takes.
        maximum: u64,
    },
    /// No epoch follows `epoch`, the largest number of an epoch's 42 bits.
    EpochLimit {
        /// The last epoch, 2^42 − 1.
        epoch: u64,
    },
    /// A request names `epoch`, which is ahead of `latest`, the furthest epoch the sequencer has reached for it.
    EpochAhead {
        /// The epoch the request names.
        epoch: u64,
        /// The furthest epoch the request may name.
        latest: u64,
    },
    /// The time `time_us` lies before the first close the sequencer holds, at `first_us`, or before any close.
    BeforeFirstClose {
        /// The time mapped, in microseconds.
        time_us: u64,
        /// The first close's recorded time in microseconds; `None` when no epoch has closed.
        first_us: Option<u64>,
    },
    /// A multi-partition transaction names no partition: its partition list is empty.
    NoPartitions,
    /// A multi-partition transaction names `partition` more than once.
    DuplicatePartition {
        /// The partition named twice.
        partition: u32,
    },
    /// `partition` is not subscribed to the sequencer's closes.
    NotSubscribed {
        /// The partition named.
        partition: u32,
    },
    /// The closes after `after` are no longer held; the oldest held is `oldest`.
    CatchUpTrimmed {
        /// The last epoch delivered to the subscriber.
        after: u64,
        /// The oldest epoch whose close is still held.
        oldest: u64,
    },
    /// A catalog version of `namespace` built on version `predecessor`, while version `current` is the namespace's
    /// latest.
    StaleCatalog {
        /// The namespace whose catalog it is.
        namespace: u64,
        /// The version the offer is built on.
        predecessor: u64,
        /// The namespace's latest version.
        current: u64,
    },
    /// A close announces version `version` of `namespace`, which the metadata group holds no pending version of.
    NotPending {
        /// The namespace whose catalog it is.
        namespace: u64,
        /// The version announced.
        version: u64,
    },
    /// The catalog version of `namespace` in effect at `epoch` is no longer retained: no partition's untrimmed log
    /// names it.
    CatalogReleased {
        /// The namespace whose catalog it is.
        namespace: u64,
        /// The epoch asked for.
        epoch: u64,
    },
    /// A block of `size` sequences was asked for; a lease takes from 1 to `max`, the setting `block_size_max`.
    BlockSize {
        /// The sequences asked for.
        size: u64,
        /// The most sequences a block holds.
        max: u64,
    },
    /// The sequences of the type with prefix `prefix` are spent: no block of the size asked for lies below 2^64.
    SequencesSpent {
        /// The type's prefix.
        prefix: u16,
    },
    /// The metadata group holds no placement map: it is initialised at the cluster's first start.
    NoPlacement,
    /// A command's recorded decision contradicts the state it is applied to: `what` does not hold.
    Contradicts {
        /// The rule the decision breaks.
        what: &'static str,
    },
    /// A principal's name of `length` bytes; a name holds from 1 to 255.
    PrincipalName {
        /// The name's length in bytes.
        length: usize,
    },
    /// A principal named `name` exists.
    PrincipalExists {
        /// The name.
        name: String,
    },
    /// No principal is named `name`.
    UnknownPrincipal {
        /// The name.
        name: String,
    },
    /// `name` is the last principal holding the administrator role, which a cluster keeps.
    LastAdministrator {
        /// The principal's name.
        name: String,
    },
    /// The bootstrap principal is not created: it needs `what`.
    Bootstrap {
        /// What the bootstrap needs and did not have.
        what: &'static str,
    },
    /// The session `session` is registered already.
    SessionExists {
        /// The session's 128-bit identity.
        session: u128,
    },
    /// The session `session` is not in the registry: it was evicted, or never registered.
    SessionEvicted {
        /// The session's 128-bit identity.
        session: u128,
    },
    /// `what` holds `count`, more than its limit `max`.
    TooMany {
        /// What was counted.
        what: &'static str,
        /// Its count.
        count: usize,
        /// The most it holds.
        max: usize,
    },
    /// `what` holds `capacity` entries, its capacity, so it takes no more.
    Full {
        /// The table that is full.
        what: &'static str,
        /// Its capacity.
        capacity: usize,
    },
    /// Encoded bytes do not hold `what` at byte `offset`.
    Malformed {
        /// What the bytes should hold there.
        what: &'static str,
        /// The byte offset of the refused value.
        offset: usize,
    },
    /// The round at log position `position` contradicts the rounds before it: `what`.
    ReplayDiverged {
        /// The round's position.
        position: u64,
        /// The rule it breaks.
        what: &'static str,
    },
    /// A snapshot taken at log position `snapshot` lies below the log's checkpoint `checkpoint`: the rounds between
    /// are trimmed.
    SnapshotBehind {
        /// The position the snapshot was taken at.
        snapshot: u64,
        /// The log's checkpoint.
        checkpoint: u64,
    },
    /// The ordered log refused the operation.
    Log(LogError),
    /// An invariant of UltraGraphDB does not hold: a bug.
    Invariant(InvariantError),
}

impl ClusterError {
    /// A partition ID of 0.
    pub fn zero_partition() -> Self {
        Self(ClusterErrorEnum::ZeroPartition)
    }

    /// An even map over `count` partitions, 0 or more than `max`.
    pub fn partition_count(count: u32, max: u32) -> Self {
        Self(ClusterErrorEnum::PartitionCount { count, max })
    }

    /// The bucket range `start..end` is empty or reaches past the last bucket.
    pub fn bucket_range(start: u32, end: u32) -> Self {
        Self(ClusterErrorEnum::BucketRange { start, end })
    }

    /// A change leaves `bucket` unassigned.
    pub fn unassigned(bucket: u32) -> Self {
        Self(ClusterErrorEnum::Unassigned { bucket })
    }

    /// A change assigns `bucket` twice.
    pub fn assigned_twice(bucket: u32) -> Self {
        Self(ClusterErrorEnum::AssignedTwice { bucket })
    }

    /// A change from `epoch` does not follow the latest, from `latest`.
    pub fn change_epoch(epoch: u64, latest: u64) -> Self {
        Self(ClusterErrorEnum::ChangeEpoch { epoch, latest })
    }

    /// `partition` holds no bucket in the latest layout.
    pub fn unknown_partition(partition: u32) -> Self {
        Self(ClusterErrorEnum::UnknownPartition { partition })
    }

    /// The setting `setting` is `value`, below `bound`, which is `minimum`.
    pub fn setting_below(
        setting: &'static str,
        value: u64,
        bound: &'static str,
        minimum: u64,
    ) -> Self {
        Self(ClusterErrorEnum::SettingBelow {
            setting,
            value,
            bound,
            minimum,
        })
    }

    /// The setting `setting` is `value`, above `maximum`.
    pub fn setting_above(setting: &'static str, value: u64, maximum: u64) -> Self {
        Self(ClusterErrorEnum::SettingAbove {
            setting,
            value,
            maximum,
        })
    }

    /// No epoch follows `epoch`.
    pub fn epoch_limit(epoch: u64) -> Self {
        Self(ClusterErrorEnum::EpochLimit { epoch })
    }

    /// `epoch` is ahead of `latest`.
    pub fn epoch_ahead(epoch: u64, latest: u64) -> Self {
        Self(ClusterErrorEnum::EpochAhead { epoch, latest })
    }

    /// `time_us` lies before the first close, at `first_us`, or before any.
    pub fn before_first_close(time_us: u64, first_us: Option<u64>) -> Self {
        Self(ClusterErrorEnum::BeforeFirstClose { time_us, first_us })
    }

    /// A multi-partition transaction names no partition.
    pub fn no_partitions() -> Self {
        Self(ClusterErrorEnum::NoPartitions)
    }

    /// A multi-partition transaction names `partition` twice.
    pub fn duplicate_partition(partition: u32) -> Self {
        Self(ClusterErrorEnum::DuplicatePartition { partition })
    }

    /// `partition` is not subscribed.
    pub fn not_subscribed(partition: u32) -> Self {
        Self(ClusterErrorEnum::NotSubscribed { partition })
    }

    /// The closes after `after` are no longer held; the oldest is `oldest`.
    pub fn catch_up_trimmed(after: u64, oldest: u64) -> Self {
        Self(ClusterErrorEnum::CatchUpTrimmed { after, oldest })
    }

    /// A version of `namespace` built on `predecessor` while `current` is the latest.
    pub fn stale_catalog(namespace: u64, predecessor: u64, current: u64) -> Self {
        Self(ClusterErrorEnum::StaleCatalog {
            namespace,
            predecessor,
            current,
        })
    }

    /// A close announces version `version` of `namespace`, which is not pending.
    pub fn not_pending(namespace: u64, version: u64) -> Self {
        Self(ClusterErrorEnum::NotPending { namespace, version })
    }

    /// The version of `namespace` in effect at `epoch` is no longer retained.
    pub fn catalog_released(namespace: u64, epoch: u64) -> Self {
        Self(ClusterErrorEnum::CatalogReleased { namespace, epoch })
    }

    /// A block of `size` sequences, outside 1 to `max`.
    pub fn block_size(size: u64, max: u64) -> Self {
        Self(ClusterErrorEnum::BlockSize { size, max })
    }

    /// The sequences of the type `prefix` are spent.
    pub fn sequences_spent(prefix: u16) -> Self {
        Self(ClusterErrorEnum::SequencesSpent { prefix })
    }

    /// The metadata group holds no placement map.
    pub fn no_placement() -> Self {
        Self(ClusterErrorEnum::NoPlacement)
    }

    /// A command's decision contradicts the state: `what` does not hold.
    pub fn contradicts(what: &'static str) -> Self {
        Self(ClusterErrorEnum::Contradicts { what })
    }

    /// A principal's name of `length` bytes, outside 1 to 255.
    pub fn principal_name(length: usize) -> Self {
        Self(ClusterErrorEnum::PrincipalName { length })
    }

    /// A principal named `name` exists.
    pub fn principal_exists(name: &str) -> Self {
        Self(ClusterErrorEnum::PrincipalExists {
            name: name.to_string(),
        })
    }

    /// No principal is named `name`.
    pub fn unknown_principal(name: &str) -> Self {
        Self(ClusterErrorEnum::UnknownPrincipal {
            name: name.to_string(),
        })
    }

    /// `name` is the last administrator.
    pub fn last_administrator(name: &str) -> Self {
        Self(ClusterErrorEnum::LastAdministrator {
            name: name.to_string(),
        })
    }

    /// The bootstrap principal needs `what`.
    pub fn bootstrap(what: &'static str) -> Self {
        Self(ClusterErrorEnum::Bootstrap { what })
    }

    /// The session `session` is registered already.
    pub fn session_exists(session: u128) -> Self {
        Self(ClusterErrorEnum::SessionExists { session })
    }

    /// The session `session` is not in the registry.
    pub fn session_evicted(session: u128) -> Self {
        Self(ClusterErrorEnum::SessionEvicted { session })
    }

    /// A snapshot at `snapshot` lies below the log's checkpoint `checkpoint`.
    pub fn snapshot_behind(snapshot: u64, checkpoint: u64) -> Self {
        Self(ClusterErrorEnum::SnapshotBehind {
            snapshot,
            checkpoint,
        })
    }

    /// `what` holds `count`, more than `max`.
    pub fn too_many(what: &'static str, count: usize, max: usize) -> Self {
        Self(ClusterErrorEnum::TooMany { what, count, max })
    }

    /// `what` is full at `capacity`.
    pub fn full(what: &'static str, capacity: usize) -> Self {
        Self(ClusterErrorEnum::Full { what, capacity })
    }

    /// Bytes do not hold `what` at `offset`.
    pub fn malformed(what: &'static str, offset: usize) -> Self {
        Self(ClusterErrorEnum::Malformed { what, offset })
    }

    /// The round at `position` breaks `what`.
    pub fn replay_diverged(position: u64, what: &'static str) -> Self {
        Self(ClusterErrorEnum::ReplayDiverged { position, what })
    }

    /// The ordered log refused with `error`.
    pub fn log(error: LogError) -> Self {
        Self(ClusterErrorEnum::Log(error))
    }

    /// The invariant violation `error`.
    pub fn invariant(error: InvariantError) -> Self {
        Self(ClusterErrorEnum::Invariant(error))
    }
}
