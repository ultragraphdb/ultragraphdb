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
            ClusterError::bucket_group_range(262_144),
            "bucket group 262144 does not fit in 18 bits; the largest is 262143",
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
