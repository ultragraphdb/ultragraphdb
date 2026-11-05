/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_error_log::LogError;

/// The placement map's constructors, each with the variant it builds; every field differs from the others.
fn placement_cases() -> [(ClusterError, ClusterErrorEnum); 7] {
    [
        (
            ClusterError::zero_partition(),
            ClusterErrorEnum::ZeroPartition,
        ),
        (
            ClusterError::partition_count(1_025, 1_024),
            ClusterErrorEnum::PartitionCount {
                count: 1_025,
                max: 1_024,
            },
        ),
        (
            ClusterError::bucket_range(7, 3),
            ClusterErrorEnum::BucketRange { start: 7, end: 3 },
        ),
        (
            ClusterError::unassigned(100),
            ClusterErrorEnum::Unassigned { bucket: 100 },
        ),
        (
            ClusterError::assigned_twice(200),
            ClusterErrorEnum::AssignedTwice { bucket: 200 },
        ),
        (
            ClusterError::change_epoch(4, 9),
            ClusterErrorEnum::ChangeEpoch {
                epoch: 4,
                latest: 9,
            },
        ),
        (
            ClusterError::unknown_partition(6),
            ClusterErrorEnum::UnknownPartition { partition: 6 },
        ),
    ]
}

/// The settings' and the sequencer's constructors, each with the variant it builds.
fn sequencer_cases() -> [(ClusterError, ClusterErrorEnum); 10] {
    [
        (
            ClusterError::setting_below("epoch_ms_max", 4, "epoch_ms_min", 5),
            ClusterErrorEnum::SettingBelow {
                setting: "epoch_ms_max",
                value: 4,
                bound: "epoch_ms_min",
                minimum: 5,
            },
        ),
        (
            ClusterError::setting_above("epoch_ms_min", 1_001, 1_000),
            ClusterErrorEnum::SettingAbove {
                setting: "epoch_ms_min",
                value: 1_001,
                maximum: 1_000,
            },
        ),
        (
            ClusterError::epoch_limit(11),
            ClusterErrorEnum::EpochLimit { epoch: 11 },
        ),
        (
            ClusterError::epoch_ahead(13, 12),
            ClusterErrorEnum::EpochAhead {
                epoch: 13,
                latest: 12,
            },
        ),
        (
            ClusterError::before_first_close(500, Some(1_000)),
            ClusterErrorEnum::BeforeFirstClose {
                time_us: 500,
                first_us: Some(1_000),
            },
        ),
        (
            ClusterError::before_first_close(500, None),
            ClusterErrorEnum::BeforeFirstClose {
                time_us: 500,
                first_us: None,
            },
        ),
        (
            ClusterError::no_partitions(),
            ClusterErrorEnum::NoPartitions,
        ),
        (
            ClusterError::duplicate_partition(3),
            ClusterErrorEnum::DuplicatePartition { partition: 3 },
        ),
        (
            ClusterError::not_subscribed(8),
            ClusterErrorEnum::NotSubscribed { partition: 8 },
        ),
        (
            ClusterError::catch_up_trimmed(3, 9),
            ClusterErrorEnum::CatchUpTrimmed {
                after: 3,
                oldest: 9,
            },
        ),
    ]
}

/// The bounds', the damage's and the carried errors' constructors, each with the variant it builds.
fn other_cases() -> [(ClusterError, ClusterErrorEnum); 6] {
    [
        (
            ClusterError::too_many("catalog versions", 257, 256),
            ClusterErrorEnum::TooMany {
                what: "catalog versions",
                count: 257,
                max: 256,
            },
        ),
        (
            ClusterError::full("subscribers", 1_024),
            ClusterErrorEnum::Full {
                what: "subscribers",
                capacity: 1_024,
            },
        ),
        (
            ClusterError::malformed("a kind byte", 0),
            ClusterErrorEnum::Malformed {
                what: "a kind byte",
                offset: 0,
            },
        ),
        (
            ClusterError::replay_diverged(5, "a close one after the last"),
            ClusterErrorEnum::ReplayDiverged {
                position: 5,
                what: "a close one after the last",
            },
        ),
        (
            ClusterError::log(LogError::busy(8)),
            ClusterErrorEnum::Log(LogError::busy(8)),
        ),
        (
            ClusterError::invariant(InvariantError::violated("closes rise by one")),
            ClusterErrorEnum::Invariant(InvariantError::violated("closes rise by one")),
        ),
    ]
}

/// The metadata group's constructors of catalog, block and placement refusals and the snapshot's, each with the
/// variant it builds.
fn metadata_cases() -> [(ClusterError, ClusterErrorEnum); 8] {
    [
        (
            ClusterError::stale_catalog(7, 2, 3),
            ClusterErrorEnum::StaleCatalog {
                tenant: 7,
                predecessor: 2,
                current: 3,
            },
        ),
        (
            ClusterError::not_pending(7, 4),
            ClusterErrorEnum::NotPending {
                tenant: 7,
                version: 4,
            },
        ),
        (
            ClusterError::catalog_released(7, 15),
            ClusterErrorEnum::CatalogReleased {
                tenant: 7,
                epoch: 15,
            },
        ),
        (
            ClusterError::block_size(1_001, 1_000),
            ClusterErrorEnum::BlockSize {
                size: 1_001,
                max: 1_000,
            },
        ),
        (
            ClusterError::sequences_spent(1),
            ClusterErrorEnum::SequencesSpent { prefix: 1 },
        ),
        (ClusterError::no_placement(), ClusterErrorEnum::NoPlacement),
        (
            ClusterError::contradicts("a block starts at its type's high-water mark"),
            ClusterErrorEnum::Contradicts {
                what: "a block starts at its type's high-water mark",
            },
        ),
        (
            ClusterError::snapshot_behind(3, 4),
            ClusterErrorEnum::SnapshotBehind {
                snapshot: 3,
                checkpoint: 4,
            },
        ),
    ]
}

/// The metadata group's constructors of principal and session refusals, each with the variant it builds.
fn access_cases() -> [(ClusterError, ClusterErrorEnum); 7] {
    [
        (
            ClusterError::principal_name(256),
            ClusterErrorEnum::PrincipalName { length: 256 },
        ),
        (
            ClusterError::principal_exists("ann"),
            ClusterErrorEnum::PrincipalExists {
                name: "ann".to_string(),
            },
        ),
        (
            ClusterError::unknown_principal("bob"),
            ClusterErrorEnum::UnknownPrincipal {
                name: "bob".to_string(),
            },
        ),
        (
            ClusterError::last_administrator("root"),
            ClusterErrorEnum::LastAdministrator {
                name: "root".to_string(),
            },
        ),
        (
            ClusterError::bootstrap("a password"),
            ClusterErrorEnum::Bootstrap { what: "a password" },
        ),
        (
            ClusterError::session_exists(10),
            ClusterErrorEnum::SessionExists { session: 10 },
        ),
        (
            ClusterError::session_evicted(11),
            ClusterErrorEnum::SessionEvicted { session: 11 },
        ),
    ]
}

#[test]
fn test_each_constructor_builds_its_variant() {
    for (error, variant) in placement_cases()
        .into_iter()
        .chain(sequencer_cases())
        .chain(other_cases())
        .chain(metadata_cases())
        .chain(access_cases())
    {
        assert_eq!(error.0, variant);
    }
}

#[test]
fn test_errors_of_one_variant_differ_by_their_fields() {
    assert_ne!(ClusterError::unassigned(100), ClusterError::unassigned(101));
    assert_ne!(
        ClusterError::unassigned(100),
        ClusterError::assigned_twice(100)
    );
    assert_ne!(
        ClusterError::epoch_ahead(13, 12),
        ClusterError::epoch_ahead(12, 13)
    );
}
