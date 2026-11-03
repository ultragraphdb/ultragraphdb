/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_error_log::LogError;
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_a_carried_log_error_is_the_source() -> TestResult {
    let error = ClusterError::log(LogError::busy(8));
    let source = error.source().context("the log error as the source")?;
    assert_eq!(source.to_string(), LogError::busy(8).to_string());
    Ok(())
}

#[test]
fn test_a_carried_invariant_error_is_the_source() -> TestResult {
    let error = ClusterError::invariant(InvariantError::violated("closes rise by one"));
    let source = error
        .source()
        .context("the invariant error as the source")?;
    assert_eq!(source.to_string(), "invariant violated: closes rise by one");
    Ok(())
}

#[test]
fn test_a_refusal_of_its_own_has_no_source() {
    // Every variant but the two carried ones, 35: each is a refusal of the cluster's own, with nothing beneath it.
    let own = [
        ClusterError::zero_partition(),
        ClusterError::partition_count(0, 1_024),
        ClusterError::bucket_range(7, 3),
        ClusterError::unassigned(100),
        ClusterError::assigned_twice(200),
        ClusterError::change_epoch(4, 9),
        ClusterError::unknown_partition(6),
        ClusterError::setting_below("epoch_ms_min", 0, "its minimum", 1),
        ClusterError::setting_above("epoch_ms_max", 10_001, 10_000),
        ClusterError::epoch_limit(4_398_046_511_103),
        ClusterError::epoch_ahead(13, 12),
        ClusterError::before_first_close(500, None),
        ClusterError::no_partitions(),
        ClusterError::duplicate_partition(3),
        ClusterError::not_subscribed(8),
        ClusterError::catch_up_trimmed(3, 9),
        ClusterError::stale_catalog(1, 2, 3),
        ClusterError::not_pending(7, 4),
        ClusterError::catalog_released(7, 15),
        ClusterError::block_size(0, 1_000),
        ClusterError::sequences_spent(1),
        ClusterError::no_placement(),
        ClusterError::contradicts("a block starts at its type's high-water mark"),
        ClusterError::principal_name(0),
        ClusterError::principal_exists("ann"),
        ClusterError::unknown_principal("bob"),
        ClusterError::last_administrator("root"),
        ClusterError::bootstrap("a password"),
        ClusterError::session_exists(10),
        ClusterError::session_evicted(11),
        ClusterError::too_many("catalog versions", 257, 256),
        ClusterError::full("subscribers", 1_024),
        ClusterError::malformed("a kind byte", 0),
        ClusterError::replay_diverged(5, "a close one after the last"),
        ClusterError::snapshot_behind(3, 4),
    ];
    for error in own {
        assert!(error.source().is_none(), "{error:?} has a source");
    }
}
