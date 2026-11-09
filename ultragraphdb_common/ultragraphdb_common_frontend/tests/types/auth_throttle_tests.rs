/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

use ultragraphdb_common_frontend::{AuthThrottle, LoginLimits};
use ultragraphdb_tool_bdd::{Context, TestResult};

const PEER: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const OTHER: IpAddr = IpAddr::V4(Ipv4Addr::BROADCAST);
/// The failures before a throttle, and past it before a ban, of these tests.
const BEFORE_THROTTLE: u32 = 5;
const THROTTLED: u32 = 3;
const BAN: Duration = Duration::from_secs(60);
/// The sources the throttle holds at most.
const SOURCES: u128 = 65_536;
/// The longest ban the throttle holds, as it documents it: a century.
const CENTURY: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);

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

/// A throttle banning a source at its second failure, so a full table fills with bans cheaply.
fn quick() -> AuthThrottle {
    AuthThrottle::new(LoginLimits::new(1, 1, BAN))
}

/// The source `n`: the IPv6 /64 `n`.
fn source(n: u128) -> IpAddr {
    IpAddr::V6(Ipv6Addr::from(n << 64))
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

/// A ban set longer than a century lasts a century: the source is banned until a century on, and admitted from then.
#[test]
fn test_a_ban_lasts_at_most_a_century() -> TestResult {
    let mut logins = AuthThrottle::new(LoginLimits::new(BEFORE_THROTTLE, THROTTLED, Duration::MAX));
    let now = start();
    let banned = (0..BEFORE_THROTTLE + THROTTLED)
        .map(|_| logins.failed(PEER, now))
        .collect::<Vec<_>>();
    assert_eq!(banned.last(), Some(&true), "banned by the last failure");
    let ends = now.checked_add(CENTURY).context("a century on")?;
    let before = ends
        .checked_sub(Duration::from_millis(1))
        .context("just before a century on")?;
    assert!(!logins.admits(PEER, before), "banned until a century on");
    assert!(logins.admits(PEER, ends), "admitted a century on");
    Ok(())
}

/// A ban of no time bans nothing: the failure reaching the limit reports no ban, and the source is admitted at once.
#[test]
fn test_a_ban_of_no_time_bans_nothing() {
    let mut logins =
        AuthThrottle::new(LoginLimits::new(BEFORE_THROTTLE, THROTTLED, Duration::ZERO));
    let now = start();
    let banned = (0..BEFORE_THROTTLE + THROTTLED)
        .map(|_| logins.failed(PEER, now))
        .collect::<Vec<_>>();
    assert!(!banned.contains(&true), "never reported banned");
    assert!(logins.admits(PEER, now));
}

/// While the table has room, a source failing for the first time forgets no other source's failures.
#[test]
fn test_a_new_source_forgets_no_other_while_the_table_has_room() {
    let mut logins = throttle();
    let now = start();
    for _ in 0..=BEFORE_THROTTLE {
        let _banned = logins.failed(PEER, now);
    }
    let waits = logins.delay(PEER);
    assert_eq!(
        waits,
        AuthThrottle::delay_after(1),
        "one failure past the threshold"
    );
    assert!(!logins.failed(OTHER, now));
    assert_eq!(logins.delay(PEER), waits, "its failures still counted");
}

/// A table full of sources counting failures forgets them to count a new source: the new one counted from its first failure,
/// the others from none again.
#[test]
fn test_a_full_table_forgets_counting_sources_for_a_new_one() {
    let mut logins = quick();
    let now = start();
    assert!(!logins.failed(PEER, now));
    for n in 1..SOURCES {
        let _banned = logins.failed(source(n), now);
    }
    assert!(!logins.failed(OTHER, now), "counted in the room made");
    assert!(logins.failed(OTHER, now), "banned at its second failure");
    assert!(!logins.failed(PEER, now), "forgotten to make room");
}

/// A table full of bans makes room from a ban only once it has ended: a new source goes uncounted while every ban runs, and is
/// counted from the moment one ends.
#[test]
fn test_a_full_table_makes_room_from_a_ban_once_it_ends() -> TestResult {
    let mut logins = quick();
    let first = start();
    let later = first
        .checked_add(Duration::from_millis(1))
        .context("a moment on")?;
    assert!(!logins.failed(PEER, first));
    assert!(logins.failed(PEER, first), "the first ban");
    // Every first failure before any second: a mutant making room on each new source then finds no ban to keep, and the fill
    // stays linear.
    for n in 1..SOURCES {
        let _banned = logins.failed(source(n), later);
    }
    assert!(
        (1..SOURCES).all(|n| logins.failed(source(n), later)),
        "every other source banned after the first"
    );
    let ends = first.checked_add(BAN).context("the first ban's end")?;
    let before = ends
        .checked_sub(Duration::from_millis(1))
        .context("just before the first ban's end")?;
    assert!(!logins.failed(OTHER, before), "uncounted");
    assert!(!logins.failed(OTHER, before), "still uncounted");
    assert!(
        !logins.failed(OTHER, ends),
        "counted once the first ban ends"
    );
    assert!(logins.failed(OTHER, ends), "banned at its second failure");
    Ok(())
}

/// The limits the settings default to: 50 failures, 20 throttled, a ban of 24 hours.
#[test]
fn test_the_limits_default_as_the_settings_say() {
    let limits = LoginLimits::default();
    assert_eq!(limits.before_throttle(), 50);
    assert_eq!(limits.throttled_before_ban(), 20);
    assert_eq!(limits.ban(), Duration::from_secs(86_400));
}
