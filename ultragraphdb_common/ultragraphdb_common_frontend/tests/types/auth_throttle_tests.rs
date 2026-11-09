/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

use ultragraphdb_common_frontend::AuthThrottle;

const PEER: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const OTHER: IpAddr = IpAddr::V4(Ipv4Addr::BROADCAST);
const INTERVAL: Duration = Duration::from_secs(900);
const NAME: &str = "ann";
/// The consecutive failures `SECURITY.md` refuses a principal past.
const FAILURES: u32 = 100;
/// The addresses the throttle holds at most.
const ADDRESSES: u128 = 65_536;
/// The names the throttle holds at most.
const NAMES: usize = 65_536;

/// The `n`th IPv6 address, none of them `PEER` or `OTHER`.
fn v6(n: u128) -> IpAddr {
    IpAddr::V6(Ipv6Addr::from(n))
}

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
    assert!(first > Duration::ZERO, "a failure costs the next attempt");
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

/// Another address's failure leaves an address's count; a full table keeps counting an address it holds, and starts over for
/// one it does not.
#[test]
fn test_a_full_address_table_starts_over_only_for_a_new_address() {
    let throttle = AuthThrottle::new(INTERVAL);
    throttle.failed(PEER);
    throttle.failed(OTHER);
    assert_eq!(
        throttle.delay(PEER),
        AuthThrottle::delay_after(1),
        "another address's failure leaves it"
    );
    for other in 2..ADDRESSES {
        throttle.failed(v6(other));
    }
    throttle.failed(PEER);
    assert_eq!(
        throttle.delay(PEER),
        AuthThrottle::delay_after(2),
        "an address the full table holds"
    );
    throttle.failed(v6(ADDRESSES));
    assert_eq!(throttle.delay(PEER), Duration::ZERO, "started over");
    assert_eq!(throttle.delay(v6(ADDRESSES)), AuthThrottle::delay_after(1));
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

/// Another name's failure leaves a name's count.
#[test]
fn test_a_names_count_holds_while_another_name_fails() {
    let (throttle, now) = (AuthThrottle::new(INTERVAL), start());
    throttle.name_failed(NAME, now);
    throttle.name_failed(NAME, now);
    throttle.name_failed("bob", now);
    assert_eq!(throttle.name_delay(NAME), AuthThrottle::delay_after(2));
}

/// A full table of names counting failures keeps counting a name it holds; a new name frees the counting names and is counted.
#[test]
fn test_a_full_name_table_counts_a_held_name_and_frees_room_for_a_new_one() {
    let (throttle, now) = (AuthThrottle::new(INTERVAL), start());
    throttle.name_failed(NAME, now);
    throttle.name_failed(NAME, now);
    for other in 1..NAMES {
        throttle.name_failed(&format!("guess-{other}"), now);
    }
    throttle.name_failed(NAME, now);
    assert_eq!(
        throttle.name_delay(NAME),
        AuthThrottle::delay_after(3),
        "a name the full table holds"
    );
    throttle.name_failed("new", now);
    assert_eq!(throttle.name_delay("new"), AuthThrottle::delay_after(1));
    assert_eq!(throttle.name_delay(NAME), Duration::ZERO, "freed for it");
}

/// A refusal ends at its interval's end: a full table of refusals ending as a new name fails frees them for it.
#[test]
fn test_refusals_ending_make_room_in_a_full_name_table() {
    let (throttle, now) = (AuthThrottle::new(INTERVAL), start());
    for other in 0..NAMES {
        let guess = format!("guess-{other}");
        for _ in 0..FAILURES {
            throttle.name_failed(&guess, now);
        }
    }
    // While every refusal runs, a new name goes uncounted.
    throttle.name_failed(NAME, now);
    assert_eq!(throttle.name_delay(NAME), Duration::ZERO);
    let ended = now + INTERVAL;
    throttle.name_failed(NAME, ended);
    assert_eq!(throttle.name_delay(NAME), AuthThrottle::delay_after(1));
}

/// Every endpoint core holds a clone of one throttle, so failures counted on one hold back the next attempt on another, and a
/// name refused on one is refused on all: a prober's connections spread over the cores count as one prober's (design D14).
#[test]
fn test_cores_sharing_a_throttle_count_failures_together() {
    fn shared_across_threads<T: Send + Sync>(_: &T) {}
    let first = AuthThrottle::new(INTERVAL);
    let second = first.clone();
    shared_across_threads(&first);
    first.failed(PEER);
    first.failed(PEER);
    assert_eq!(second.delay(PEER), AuthThrottle::delay_after(2));
    let now = start();
    for _ in 0..FAILURES {
        first.name_failed(NAME, now);
    }
    assert!(second.name_refused(NAME, now), "refused on every core");
    second.succeeded(PEER);
    assert_eq!(first.delay(PEER), std::time::Duration::ZERO);
}
