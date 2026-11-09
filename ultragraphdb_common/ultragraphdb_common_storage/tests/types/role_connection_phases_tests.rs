/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_common_service::PHASES_QUIET_US;
use ultragraphdb_common_storage::RoleConnectionPhases;
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// A window whose times count from the runtime's clock now, as the storage role starts one: its timer's first tick is now.
async fn phases() -> RoleConnectionPhases {
    RoleConnectionPhases::new(compio::time::interval(Duration::from_secs(1)).tick().await)
}

/// A request recorded is counted, a clone shares the window, and a report before the window is due leaves it as it is.
#[test]
fn test_requests_counted_into_one_window_its_clones_share() -> TestResult {
    Runtime::new()?.block_on(async {
        let phases = phases().await;
        let shared = phases.clone();
        phases.record(10, 110);
        shared.record(20, 520);
        assert_eq!(phases.count(), 2);
        // Due a quiet second after its last request, which the window's clock is not yet past.
        phases.report(1);
        assert_eq!(shared.count(), 2, "not reported, so not cleared");
        Ok(())
    })
}

/// A window whose last request ended a quiet second ago is due: a report logs it and starts a new one.
#[test]
fn test_a_due_window_is_reported_and_starts_anew() -> TestResult {
    Runtime::new()?.block_on(async {
        let now = compio::time::interval(Duration::from_secs(1)).tick().await;
        let quiet = Duration::from_micros(PHASES_QUIET_US);
        let origin = now
            .checked_sub(quiet + quiet)
            .ok_or_else(|| TestError::new("the clock starts too late to look back".to_string()))?;
        let phases = RoleConnectionPhases::new(origin);
        phases.record(10, 110);
        phases.report(1);
        assert_eq!(phases.count(), 0, "reported, so cleared");
        Ok(())
    })
}

/// An answer taken before its read, as a clock read twice cannot give, counts as no time rather than wrapping.
#[test]
fn test_an_answer_before_its_read_counts_as_no_time() -> TestResult {
    Runtime::new()?.block_on(async {
        let phases = phases().await;
        phases.record(500, 100);
        assert_eq!(phases.count(), 1);
        Ok(())
    })
}
