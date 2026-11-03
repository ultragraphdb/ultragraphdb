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
    assert!(ClusterError::unassigned(100).source().is_none());
    assert!(ClusterError::no_partitions().source().is_none());
    assert!(ClusterError::stale_catalog(1, 2, 3).source().is_none());
    assert!(ClusterError::snapshot_behind(3, 4).source().is_none());
}
