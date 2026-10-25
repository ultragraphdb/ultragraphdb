/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::time::Duration;

use ultragraphdb_tool_bench::{Clock, Pacer};

/// A clock the test moves by hand.
struct FakeClock(Cell<Duration>);

impl Clock for FakeClock {
    fn elapsed(&self) -> Duration {
        self.0.get()
    }
}

#[test]
fn test_the_pacer_releases_the_commits_due_at_the_offered_rate() {
    let clock = FakeClock(Cell::new(Duration::ZERO));
    let mut pacer = Pacer::new(14_500);
    assert_eq!(pacer.take_due(&clock), 0);
    clock.0.set(Duration::from_millis(1));
    assert_eq!(pacer.take_due(&clock), 14);
    clock.0.set(Duration::from_millis(2));
    assert_eq!(pacer.take_due(&clock), 15);
    // The clock standing still releases nothing more.
    assert_eq!(pacer.take_due(&clock), 0);
    clock.0.set(Duration::from_secs(1));
    assert_eq!(pacer.take_due(&clock), 14_500 - 29);
    assert_eq!(pacer.released(), 14_500);
}

#[test]
fn test_the_pacer_names_when_the_next_commit_is_due() {
    let clock = FakeClock(Cell::new(Duration::from_micros(150)));
    let mut pacer = Pacer::new(50_000);
    assert_eq!(pacer.take_due(&clock), 7);
    // Commit 7 is due at 7 / 50,000 s.
    assert_eq!(pacer.next_due(), Duration::from_micros(160));
}

#[test]
fn test_a_rate_of_zero_releases_nothing() {
    let clock = FakeClock(Cell::new(Duration::from_secs(10)));
    let mut pacer = Pacer::new(0);
    assert_eq!(pacer.take_due(&clock), 0);
    assert_eq!(pacer.next_due(), Duration::MAX);
}
