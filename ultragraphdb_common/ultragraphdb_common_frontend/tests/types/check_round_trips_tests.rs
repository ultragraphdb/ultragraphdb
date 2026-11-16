/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The round trips of Check frames, timed from send to reply (design D12).

use std::time::{Duration, Instant};

use compio::runtime::Runtime;

use ultragraphdb_common_frontend::CheckRoundTrips;
use ultragraphdb_tool_bdd::TestResult;

/// A point in time from the runtime's timer, whose first tick is now: what the tests count from.
fn start() -> TestResult<Instant> {
    Ok(Runtime::new()?
        .block_on(async { compio::time::interval(Duration::from_secs(1)).tick().await }))
}

/// Three round trips of 100, 200 and 300 µs: their median is 200 µs and their mean 200 µs, within the histogram's
/// buckets; the count is 3, and a report once due starts a new window.
#[test]
fn test_round_trips_are_counted_and_reported() -> TestResult {
    let origin = start()?;
    let trips = CheckRoundTrips::new(origin).with_core(2);
    for (at, us) in [(0u64, 100u64), (1_000, 200), (2_000, 300)] {
        let sent = origin + Duration::from_micros(at);
        trips.record(sent, sent + Duration::from_micros(us));
    }
    assert_eq!(trips.count(), 3);
    let (median, p99, mean) = trips.figures();
    assert!((190..=210).contains(&median), "{median}");
    assert!((290..=310).contains(&p99), "{p99}");
    assert!((190..=210).contains(&mean), "{mean}");
    trips.report(origin + Duration::from_secs(5));
    assert_eq!(trips.count(), 0, "a new window");
    Ok(())
}

/// A window not yet due is not reported: what it counted stays.
#[test]
fn test_a_window_not_due_stays() -> TestResult {
    let origin = start()?;
    let trips = CheckRoundTrips::new(origin);
    trips.record(origin, origin + Duration::from_micros(50));
    trips.report(origin + Duration::from_micros(60));
    assert_eq!(trips.count(), 1);
    Ok(())
}
