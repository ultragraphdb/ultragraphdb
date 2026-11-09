/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_common_frontend::{ONE_FRAME_PHASES, OneFramePhases};
use ultragraphdb_common_service::PHASES_QUIET_US;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A write's phases are the times between its frame taken, its relay and its partition's answer, in the window's order, and the
/// window is taken once due, a new one in its place.
#[test]
fn test_a_writes_phases_are_taken_once_due() -> TestResult {
    Runtime::new()?.block_on(async {
        // The clock is the runtime's: its timer's first tick is now.
        let origin = compio::time::interval(Duration::from_secs(1)).tick().await;
        let phases = OneFramePhases::new(origin);
        let began = origin + Duration::from_millis(5);
        let relayed = began + Duration::from_micros(300);
        let answered = relayed + Duration::from_micros(2_000);
        phases.record(began, relayed, answered);
        let quiet = Duration::from_micros(PHASES_QUIET_US);
        assert!(phases.take_due(answered).is_none(), "not due at once");
        let shared = phases.clone();
        let window = shared
            .take_due(answered + quiet)
            .context("due a quiet second on, through a clone")?;
        assert_eq!(window.count(), 1);
        assert_eq!(window.window_us(), 2_300);
        assert_eq!(ONE_FRAME_PHASES, ["checks", "partition", "total"]);
        assert_eq!(window.figures(0).0, 300);
        assert_eq!(window.figures(1).0, 2_000);
        assert_eq!(window.figures(2).0, 2_300);
        assert!(phases.take_due(answered + quiet).is_none(), "taken");
        phases.record(began, relayed, answered);
        // Logged and taken; with no subscriber the event goes nowhere.
        phases.report(answered + quiet);
        assert!(phases.take_due(answered + quiet).is_none(), "reported");
        Ok(())
    })
}
