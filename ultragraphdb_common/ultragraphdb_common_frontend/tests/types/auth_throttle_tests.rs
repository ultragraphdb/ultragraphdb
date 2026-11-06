/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use ultragraphdb_common_frontend::AuthThrottle;

const PEER: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const OTHER: IpAddr = IpAddr::V4(Ipv4Addr::BROADCAST);
const INTERVAL: Duration = Duration::from_secs(900);
const NAME: &str = "ann";
/// The consecutive failures `SECURITY.md` refuses a principal past.
const FAILURES: u32 = 100;

#[expect(
    clippy::disallowed_methods,
    reason = "a point in time the throttle compares with, which no simulation replays"
)]
fn start() -> Instant {
    Instant::now()
}

/// A throttle `NAME` failed `FAILURES` times on, at `now`.
fn past_its_failures(now: Instant) -> AuthThrottle {
    let throttle = AuthThrottle::new(INTERVAL);
    for _ in 0..FAILURES {
        throttle.name_failed(NAME, now);
    }
    throttle
}

#[test]
fn test_each_failure_doubles_the_wait_up_to_its_most() {
    assert_eq!(AuthThrottle::delay_after(0), Duration::ZERO);
    let (first, second) = (AuthThrottle::delay_after(1), AuthThrottle::delay_after(2));
    assert_eq!(second, first * 2);
    assert_eq!(
        AuthThrottle::delay_after(32),
        AuthThrottle::delay_after(u32::MAX)
    );
}

#[test]
fn test_a_failure_holds_its_address_and_a_success_forgets_it() {
    let throttle = AuthThrottle::new(INTERVAL);
    throttle.failed(PEER);
    throttle.failed(PEER);
    assert_eq!(throttle.delay(PEER), AuthThrottle::delay_after(2));
    assert_eq!(throttle.delay(OTHER), Duration::ZERO, "per address");
    throttle.succeeded(PEER);
    assert_eq!(throttle.delay(PEER), Duration::ZERO);
}

#[test]
fn test_a_names_failures_grow_its_wait_and_a_success_forgets_them() {
    let (throttle, now) = (AuthThrottle::new(INTERVAL), start());
    throttle.name_failed(NAME, now);
    throttle.name_failed(NAME, now);
    assert_eq!(throttle.name_delay(NAME), AuthThrottle::delay_after(2));
    assert_eq!(throttle.name_delay("bob"), Duration::ZERO, "per name");
    throttle.name_succeeded(NAME);
    assert_eq!(throttle.name_delay(NAME), Duration::ZERO);
}

#[test]
fn test_a_name_is_refused_only_at_its_hundredth_failure() {
    let (throttle, now) = (AuthThrottle::new(INTERVAL), start());
    for _ in 1..FAILURES {
        throttle.name_failed(NAME, now);
    }
    assert!(!throttle.name_refused(NAME, now));
    throttle.name_failed(NAME, now);
    assert!(throttle.name_refused(NAME, now));
    assert!(!throttle.name_refused("bob", now), "per name");
}

#[test]
fn test_a_refusal_answers_at_once_and_ends_on_its_own() {
    let now = start();
    let throttle = past_its_failures(now);
    assert_eq!(throttle.name_delay(NAME), Duration::ZERO);
    // A failure during the refusal does not extend it.
    let later = now + INTERVAL / 2;
    throttle.name_failed(NAME, later);
    assert!(throttle.name_refused(NAME, later));
    let ended = now + INTERVAL;
    assert!(!throttle.name_refused(NAME, ended));
    assert_eq!(
        throttle.name_delay(NAME),
        Duration::ZERO,
        "its count forgotten"
    );
}

/// Past the names the throttle holds at most, 65,536, a refused name keeps its refusal: only names counting failures make room.
#[test]
fn test_a_full_name_table_keeps_its_refusals() {
    let now = start();
    let throttle = past_its_failures(now);
    for other in 0..(2 * 65_536) {
        throttle.name_failed(&format!("guess-{other}"), now);
    }
    assert!(throttle.name_refused(NAME, now));
}
