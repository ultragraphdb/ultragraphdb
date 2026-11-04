/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_bench::{BenchErrorEnum, wait_out_build};

/// A build already over ends the wait at the first look; one ending at the third look takes three.
#[test]
fn test_the_wait_ends_when_the_build_does() -> TestResult {
    let mut looks = 0;
    wait_out_build(
        || {
            looks += 1;
            false
        },
        5,
        Duration::ZERO,
    )?;
    assert_eq!(looks, 1);
    let mut until_third = 0;
    wait_out_build(
        || {
            until_third += 1;
            until_third < 3
        },
        5,
        Duration::ZERO,
    )?;
    assert_eq!(until_third, 3);
    Ok(())
}

/// A build never ending is given up on after the last look, naming the time waited: looks times the pause.
#[test]
fn test_a_stuck_build_ends_the_wait_with_the_time_waited() {
    let mut looks = 0;
    let stuck = wait_out_build(
        || {
            looks += 1;
            true
        },
        3,
        Duration::from_millis(2),
    );
    assert_eq!(looks, 3);
    assert!(matches!(
        stuck.map_err(|e| e.0),
        Err(BenchErrorEnum::BuildStuck { waited_ms: 6 })
    ));
}

/// With no looks allowed, the build counts as stuck: nothing said it ended.
#[test]
fn test_no_looks_is_a_stuck_build() {
    let stuck = wait_out_build(|| false, 0, Duration::from_millis(2));
    assert!(matches!(
        stuck.map_err(|e| e.0),
        Err(BenchErrorEnum::BuildStuck { waited_ms: 0 })
    ));
}
