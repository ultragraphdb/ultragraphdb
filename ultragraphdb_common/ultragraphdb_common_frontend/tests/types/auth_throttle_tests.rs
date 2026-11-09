/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

use ultragraphdb_common_frontend::{AuthThrottle, LoginLimits};

const PEER: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const OTHER: IpAddr = IpAddr::V4(Ipv4Addr::BROADCAST);
/// The failures before a throttle, and past it before a ban, of these tests.
const BEFORE_THROTTLE: u32 = 5;
const THROTTLED: u32 = 3;
const BAN: Duration = Duration::from_secs(60);
/// The sources the throttle holds at most.
const SOURCES: u128 = 65_536;

#[expect(
    clippy::disallowed_methods,
    reason = "a point in time the throttle compares with, which no simulation replays"
)]
fn start() -> Instant {
    Instant::now()
}

fn throttle() -> AuthThrottle {
    AuthThrottle::new(LoginLimits::new(BEFORE_THROTTLE, THROTTLED, BAN))
}

/// The address `n` of the IPv6 /64 `prefix`.
fn in_64(prefix: u64, n: u64) -> IpAddr {
    IpAddr::V6(Ipv6Addr::from((u128::from(prefix) << 64) | u128::from(n)))
}

/// A source's first failures wait nothing; each past them waits a delay doubling from 100 ms, at most 30 s.
#[test]
fn test_a_source_waits_only_past_its_threshold() {
    let mut logins = throttle();
    let now = start();
    for _ in 0..BEFORE_THROTTLE {
        assert_eq!(logins.delay(PEER), Duration::ZERO);
        assert!(!logins.failed(PEER, now));
    }
    assert_eq!(logins.delay(PEER), Duration::ZERO, "the threshold reached");
    assert!(!logins.failed(PEER, now));
    assert_eq!(logins.delay(PEER), Duration::from_millis(100));
    assert!(!logins.failed(PEER, now));
    assert_eq!(logins.delay(PEER), Duration::from_millis(200));
    assert_eq!(AuthThrottle::delay_after(0), Duration::ZERO);
    assert_eq!(AuthThrottle::delay_after(9), Duration::from_millis(25_600));
    assert_eq!(AuthThrottle::delay_after(10), Duration::from_secs(30));
    assert_eq!(AuthThrottle::delay_after(u32::MAX), Duration::from_secs(30));
}

/// Past its throttled failures a source is banned: not admitted until the ban ends on its own, then admitted with its failures
/// forgotten. Another source is admitted throughout.
#[test]
fn test_a_source_past_its_throttled_failures_is_banned_until_its_ban_ends() {
    let mut logins = throttle();
    let now = start();
    let banned = (0..BEFORE_THROTTLE + THROTTLED)
        .map(|_| logins.failed(PEER, now))
        .collect::<Vec<_>>();
    assert_eq!(
        banned.iter().filter(|banned| **banned).count(),
        1,
        "banned once"
    );
    assert_eq!(banned.last(), Some(&true), "by the last failure");
    assert!(!logins.admits(PEER, now));
    assert!(!logins.admits(PEER, now + BAN - Duration::from_millis(1)));
    assert!(logins.admits(OTHER, now));
    assert!(
        !logins.failed(PEER, now),
        "a failure during a ban does not extend it"
    );
    assert!(logins.admits(PEER, now + BAN));
    assert_eq!(logins.delay(PEER), Duration::ZERO, "its failures forgotten");
}

/// A success forgets a source's failures, so its next failures start from none.
#[test]
fn test_a_success_forgets_a_sources_failures() {
    let mut logins = throttle();
    let now = start();
    for _ in 0..BEFORE_THROTTLE + THROTTLED - 1 {
        let _banned = logins.failed(PEER, now);
    }
    logins.succeeded(PEER);
    assert_eq!(logins.delay(PEER), Duration::ZERO);
    assert!(!logins.failed(PEER, now), "counting from none again");
    assert!(logins.admits(PEER, now));
}

/// Every address of one IPv6 /64 counts as one source, an IPv4-mapped address as its IPv4 one, and another /64 apart.
#[test]
fn test_an_ipv6_64_is_one_source() {
    let mut logins = throttle();
    let now = start();
    for n in 0..u64::from(BEFORE_THROTTLE + THROTTLED) {
        let _banned = logins.failed(in_64(7, n), now);
    }
    assert!(
        !logins.admits(in_64(7, 1_000), now),
        "another address of the /64"
    );
    assert!(logins.admits(in_64(8, 0), now), "another /64");
    assert_eq!(AuthThrottle::source_of(in_64(7, 99)), in_64(7, 0));
    let mapped = IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped());
    assert_eq!(AuthThrottle::source_of(mapped), PEER);
}

/// A full table makes room from sources counting failures, never from a ban still running: spending addresses lifts no ban.
#[test]
fn test_a_full_table_keeps_its_bans() {
    let mut logins = throttle();
    let now = start();
    for _ in 0..BEFORE_THROTTLE + THROTTLED {
        let _banned = logins.failed(PEER, now);
    }
    for n in 0..SOURCES {
        let _banned = logins.failed(IpAddr::V6(Ipv6Addr::from(n << 64)), now);
    }
    assert!(!logins.admits(PEER, now), "still banned");
}

/// The limits the settings default to: 50 failures, 20 throttled, a ban of 24 hours.
#[test]
fn test_the_limits_default_as_the_settings_say() {
    let limits = LoginLimits::default();
    assert_eq!(limits.before_throttle(), 50);
    assert_eq!(limits.throttled_before_ban(), 20);
    assert_eq!(limits.ban(), Duration::from_secs(86_400));
}
