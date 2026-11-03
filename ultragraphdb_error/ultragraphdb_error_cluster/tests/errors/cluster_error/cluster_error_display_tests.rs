/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_error_log::LogError;

#[test]
fn test_placement_texts_name_the_values() {
    let cases = [
        (
            ClusterError::zero_partition(),
            "partition 0 does not exist; partitions are numbered from 1",
        ),
        (
            ClusterError::partition_count(1_025, 1_024),
            "a placement map over 1025 partitions; it takes from 1 to 1024",
        ),
        (
            ClusterError::bucket_range(7, 3),
            "the bucket range 7..3 is empty or reaches past bucket 16777215",
        ),
        (
            ClusterError::unassigned(100),
            "the change leaves bucket 100 assigned to no partition",
        ),
        (
            ClusterError::assigned_twice(200),
            "the change assigns bucket 200 to more than one partition",
        ),
        (
            ClusterError::change_epoch(4, 9),
            "a change from epoch 4 does not follow the latest change, from epoch 9",
        ),
        (
            ClusterError::unknown_partition(6),
            "partition 6 holds no bucket in the latest layout",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn test_setting_texts_name_the_setting_and_the_bound() {
    let cases = [
        (
            ClusterError::setting_below("epoch_ms_min", 0, "its minimum", 1),
            "epoch_ms_min is 0, below its minimum, 1",
        ),
        (
            ClusterError::setting_below("epoch_ms_max", 4, "epoch_ms_min", 5),
            "epoch_ms_max is 4, below epoch_ms_min, 5",
        ),
        (
            ClusterError::setting_above("epoch_ms_max", 10_001, 10_000),
            "epoch_ms_max is 10001, above its maximum, 10000",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn test_sequencer_texts_name_the_values() {
    let cases = [
        (
            ClusterError::epoch_limit(4_398_046_511_103),
            "no epoch follows epoch 4398046511103, the largest a 42-bit epoch number holds",
        ),
        (
            ClusterError::epoch_ahead(13, 12),
            "epoch 13 is ahead of epoch 12, the furthest the sequencer has reached for it",
        ),
        (
            ClusterError::before_first_close(500, Some(1_000)),
            "the time 500 µs lies before the first close, at 1000 µs",
        ),
        (
            ClusterError::before_first_close(500, None),
            "the time 500 µs lies before the first close; no epoch has closed",
        ),
        (
            ClusterError::no_partitions(),
            "a multi-partition transaction names no partition: its partition list is empty",
        ),
        (
            ClusterError::duplicate_partition(3),
            "a multi-partition transaction names partition 3 twice",
        ),
        (
            ClusterError::not_subscribed(8),
            "partition 8 is not subscribed to the sequencer's closes",
        ),
        (
            ClusterError::catch_up_trimmed(3, 9),
            "the closes after epoch 3 are no longer held; the oldest held is epoch 9",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn test_bound_damage_and_carried_texts() {
    let cases = [
        (
            ClusterError::too_many("catalog versions", 257, 256),
            "catalog versions: 257, more than the limit 256".to_string(),
        ),
        (
            ClusterError::full("subscribers", 1_024),
            "subscribers holds 1024 entries, its capacity".to_string(),
        ),
        (
            ClusterError::malformed("a kind byte", 0),
            "malformed encoding: a kind byte at byte 0".to_string(),
        ),
        (
            ClusterError::replay_diverged(5, "a close one after the last"),
            "the round at log position 5 contradicts the log before it: a close one after the last"
                .to_string(),
        ),
        (
            ClusterError::log(LogError::busy(8)),
            format!("ordered log: {}", LogError::busy(8)),
        ),
        (
            ClusterError::invariant(InvariantError::violated("closes rise by one")),
            "invariant violated: closes rise by one".to_string(),
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn test_metadata_texts_name_the_values() {
    let cases = [
        (
            ClusterError::stale_catalog(7, 2, 3),
            "a catalog version of namespace 7 built on version 2, while version 3 is its latest",
        ),
        (
            ClusterError::not_pending(7, 4),
            "a close announces version 4 of namespace 7's catalog, which is not pending",
        ),
        (
            ClusterError::catalog_released(7, 15),
            "the catalog version of namespace 7 in effect at epoch 15 is no longer retained",
        ),
        (
            ClusterError::block_size(1_001, 1_000),
            "a block of 1001 sequences; a lease takes from 1 to 1000, block_size_max",
        ),
        (
            ClusterError::sequences_spent(1),
            "the sequences of type prefix 1 are spent: no block of the size asked for lies below 2^64",
        ),
        (
            ClusterError::no_placement(),
            "the metadata group holds no placement map: it is initialised at the cluster's first start",
        ),
        (
            ClusterError::contradicts("a principal takes the next identity"),
            "the command contradicts the state it is applied to: a principal takes the next identity",
        ),
        (
            ClusterError::principal_name(256),
            "a principal's name of 256 bytes; a name holds from 1 to 255",
        ),
        (
            ClusterError::principal_exists("ann"),
            "a principal named ann exists",
        ),
        (
            ClusterError::unknown_principal("bob"),
            "no principal is named bob",
        ),
        (
            ClusterError::last_administrator("root"),
            "root is the last principal holding the administrator role, which the cluster keeps",
        ),
        (
            ClusterError::bootstrap("a password in the secret file"),
            "the bootstrap principal is not created: it needs a password in the secret file",
        ),
        (
            ClusterError::session_exists(10),
            "session 0000000000000000000000000000000a is registered already",
        ),
        (
            ClusterError::session_evicted(11),
            "session 0000000000000000000000000000000b is not in the registry: it was evicted, or never registered",
        ),
        (
            ClusterError::snapshot_behind(3, 4),
            "the snapshot at log position 3 lies below the log's checkpoint 4: the rounds between are trimmed",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}
