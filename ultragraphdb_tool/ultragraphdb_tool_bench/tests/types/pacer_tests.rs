/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::time::Duration;

use ultragraphdb_tool_bench::{Clock, Pacer};

/// A clock the test moves by hand.
struct FakeClock(Cell<Duration>);

impl FakeClock {
    fn at(elapsed: Duration) -> Self {
        Self(Cell::new(elapsed))
    }
}

impl Clock for FakeClock {
    fn elapsed(&self) -> Duration {
        self.0.get()
    }
}

// Commit n is due at n / rate seconds: by hand, the commits due by t are floor(t * rate).

#[test]
fn test_the_pacer_releases_the_commits_due_at_the_offered_rate() {
    let clock = FakeClock::at(Duration::ZERO);
    let mut pacer = Pacer::new(14_500);
    assert_eq!(pacer.take_due(&clock), 0);
    // floor(0.001 * 14,500) = 14.
    clock.0.set(Duration::from_millis(1));
    assert_eq!(pacer.take_due(&clock), 14);
    // floor(0.002 * 14,500) = 29, of which 14 are out.
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
    let clock = FakeClock::at(Duration::from_micros(150));
    let mut pacer = Pacer::new(50_000);
    // floor(150e-6 * 50,000) = 7.
    assert_eq!(pacer.take_due(&clock), 7);
    // Commit 8 is due at 8 / 50,000 s.
    assert_eq!(pacer.next_due(), Duration::from_micros(160));
}

#[test]
fn test_a_rate_of_zero_releases_nothing() {
    let clock = FakeClock::at(Duration::from_secs(10));
    let mut pacer = Pacer::new(0);
    assert_eq!(pacer.take_due(&clock), 0);
    assert_eq!(pacer.released(), 0);
    assert_eq!(pacer.next_due(), Duration::MAX);
}

#[test]
fn test_a_commit_is_due_at_its_instant_and_not_a_nanosecond_before() {
    // One commit a second: commit 1 at exactly 1 s.
    let mut pacer = Pacer::new(1);
    assert_eq!(pacer.next_due(), Duration::from_secs(1));
    assert_eq!(
        pacer.take_due(&FakeClock::at(Duration::from_nanos(999_999_999))),
        0
    );
    assert_eq!(pacer.take_due(&FakeClock::at(Duration::from_secs(1))), 1);
    assert_eq!(pacer.next_due(), Duration::from_secs(2));
}

#[test]
fn test_the_next_due_instant_releases_the_next_commit_when_the_rate_does_not_divide_a_second() {
    // Three a second: commit 1 is due at 333,333,333.3 ns, so at 333,333,333 ns it is not (3 * 333,333,333 =
    // 999,999,999 < 1e9) and at 333,333,334 ns it is (3 * 333,333,334 = 1,000,000,002).
    let mut pacer = Pacer::new(3);
    assert_eq!(
        pacer.take_due(&FakeClock::at(Duration::from_nanos(333_333_333))),
        0
    );
    assert_eq!(pacer.next_due(), Duration::from_nanos(333_333_334));
    assert_eq!(pacer.take_due(&FakeClock::at(pacer.next_due())), 1);
    // Commit 2 at 666,666,666.7 ns.
    assert_eq!(pacer.next_due(), Duration::from_nanos(666_666_667));
}

#[test]
fn test_a_clock_running_back_releases_nothing_and_nothing_twice() {
    let clock = FakeClock::at(Duration::from_millis(10));
    let mut pacer = Pacer::new(1_000);
    assert_eq!(pacer.take_due(&clock), 10);
    clock.0.set(Duration::from_millis(4));
    assert_eq!(pacer.take_due(&clock), 0);
    assert_eq!(pacer.released(), 10);
    // Back at 10 ms nothing is due again; at 12 ms the two after it are.
    clock.0.set(Duration::from_millis(10));
    assert_eq!(pacer.take_due(&clock), 0);
    clock.0.set(Duration::from_millis(12));
    assert_eq!(pacer.take_due(&clock), 2);
}

#[test]
fn test_a_rate_no_run_can_meet_saturates_the_count_rather_than_wrapping() {
    // u64::MAX a second for one second is exactly u64::MAX commits, the largest count there is.
    let mut pacer = Pacer::new(u64::MAX);
    assert_eq!(
        pacer.take_due(&FakeClock::at(Duration::from_secs(1))),
        u64::MAX
    );
    assert_eq!(pacer.released(), u64::MAX);
    // Commit u64::MAX + 1 would be due at 2^64 / (2^64 - 1) s, 1 s and 5.4e-11 ns, so at 1,000,000,001 ns.
    assert_eq!(pacer.next_due(), Duration::from_nanos(1_000_000_001));
    // For three seconds the count, 3 * (2^64 - 1), is past u64::MAX: it stays at u64::MAX, where wrapping would
    // give u64::MAX - 2.
    let mut fresh = Pacer::new(u64::MAX);
    assert_eq!(
        fresh.take_due(&FakeClock::at(Duration::from_secs(3))),
        u64::MAX
    );
}

#[test]
fn test_the_largest_clock_saturates_the_count() {
    // Duration::MAX is u64::MAX s and 999,999,999 ns, about 1.8e28 ns; times u64::MAX it is past u128 as well.
    let mut pacer = Pacer::new(u64::MAX);
    assert_eq!(pacer.take_due(&FakeClock::at(Duration::MAX)), u64::MAX);
    // At one a second it is floor(u64::MAX + 0.999999999) = u64::MAX exactly; the next commit, at 2^64 s, is past
    // what a Duration of nanoseconds holds, and is named the largest one, u64::MAX ns.
    let mut one = Pacer::new(1);
    assert_eq!(one.take_due(&FakeClock::at(Duration::MAX)), u64::MAX);
    assert_eq!(one.next_due(), Duration::from_nanos(u64::MAX));
}
