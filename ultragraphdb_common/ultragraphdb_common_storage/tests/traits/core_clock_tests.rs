/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_common_storage::CoreClock;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The role's clock, an instant, reads the microseconds since it, never falling.
#[test]
fn test_an_instant_reads_the_microseconds_since_it() -> TestResult {
    Runtime::new()?.block_on(async {
        let now = compio::time::interval(Duration::from_secs(1)).tick().await;
        let gap = Duration::from_millis(5);
        let began = now.checked_sub(gap).context("an instant before now")?;
        let first = began.now_us();
        assert!(
            first >= u64::try_from(gap.as_micros())?,
            "{first} microseconds"
        );
        assert!(began.now_us() >= first);
        Ok(())
    })
}
