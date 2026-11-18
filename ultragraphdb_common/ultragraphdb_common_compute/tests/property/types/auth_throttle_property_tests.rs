/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The authentication throttle: laws L5.11 and L5.12 of `notes/laws.md`, against a model of `PROTOCOL.md` §11.9 and
//! client-endpoint, "Connections authenticate against the metadata group's principals".

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

use compio::runtime::Runtime;
use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_compute::{AuthThrottle, LoginLimits};

const BAN: Duration = Duration::from_secs(60);

fn start() -> Option<Instant> {
    let runtime = Runtime::new().ok()?;
    Some(runtime.block_on(async { compio::time::interval(Duration::from_secs(1)).tick().await }))
}

/// The model's delay after `failures` consecutive failures with `before` the throttle's threshold: none up to it, then
/// 100 ms doubling to at most 30 s.
fn delay_of(failures: u32, before: u32) -> Duration {
    let past = failures.saturating_sub(before);
    if past == 0 {
        return Duration::ZERO;
    }
    let mut delay = Duration::from_millis(100);
    for _ in 1..past {
        delay = (delay * 2).min(Duration::from_secs(30));
    }
    delay
}

/// L5.11: a source's delay and ban follow from its consecutive failures and the thresholds alone; a success clears its
/// count; an Error 17, which reaches the throttle as nothing, counts nothing; another source is never delayed.
#[hegel::test(test_cases = 60)]
fn test_the_throttle_agrees_with_its_model(tc: TestCase) {
    let Some(origin) = start() else {
        return;
    };
    let before = tc.draw(gs::integers::<u32>().min_value(1).max_value(4));
    let throttled = tc.draw(gs::integers::<u32>().min_value(1).max_value(4));
    let mut throttle = AuthThrottle::new(LoginLimits::new(before, throttled, BAN));
    let source = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    let other = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
    let (mut failures, mut banned_until, mut now) = (0u32, None::<Instant>, origin);
    for _ in 0..tc.draw(gs::integers::<usize>().min_value(1).max_value(30)) {
        match tc.draw(gs::integers::<u8>().min_value(0).max_value(3)) {
            0 => {
                let banned = throttle.failed(source, now);
                let mut just_banned = false;
                if banned_until.is_none() {
                    failures += 1;
                    if failures >= before + throttled {
                        banned_until = Some(now + BAN);
                        just_banned = true;
                    }
                }
                assert_eq!(banned, just_banned);
            }
            1 => {
                throttle.succeeded(source);
                failures = 0;
                banned_until = None;
            }
            2 => {
                // An Error 17 reaches the throttle as no outcome: nothing is told.
            }
            _ => {
                now +=
                    Duration::from_secs(tc.draw(gs::integers::<u64>().min_value(0).max_value(90)));
            }
        }
        let admitted = throttle.admits(source, now);
        let model_admits = !banned_until.is_some_and(|until| now < until);
        assert_eq!(admitted, model_admits);
        if banned_until.is_some_and(|until| now >= until) {
            // A ban that has ended is forgotten with its failures.
            banned_until = None;
            failures = 0;
        }
        if banned_until.is_none() {
            assert_eq!(throttle.delay(source), delay_of(failures, before));
        }
        assert_eq!(throttle.delay(other), Duration::ZERO);
        assert!(throttle.admits(other, now));
    }
}

/// L5.12: two IPv6 addresses of one /64 are one source, two IPv4 addresses are two.
#[hegel::test(test_cases = 40)]
fn test_a_source_is_an_ipv4_address_or_an_ipv6_64(tc: TestCase) {
    let Some(now) = start() else {
        return;
    };
    let prefix = tc.draw(gs::integers::<u64>().min_value(1).max_value(u64::MAX));
    let (low, high) = (
        tc.draw(gs::integers::<u64>().min_value(0).max_value(u64::MAX)),
        tc.draw(gs::integers::<u64>().min_value(0).max_value(u64::MAX)),
    );
    let in_64 = |n: u64| IpAddr::V6(Ipv6Addr::from((u128::from(prefix) << 64) | u128::from(n)));
    let mut throttle = AuthThrottle::new(LoginLimits::new(1, 1, BAN));
    let _first = throttle.failed(in_64(low), now);
    let banned = throttle.failed(in_64(high), now);
    assert!(banned, "one source: banned at its second failure");
    assert!(!throttle.admits(in_64(low), now));
    let a = tc.draw(gs::integers::<u32>().min_value(1).max_value(u32::MAX));
    let b = tc.draw(gs::integers::<u32>().min_value(1).max_value(u32::MAX));
    tc.assume(a != b);
    let mut throttle = AuthThrottle::new(LoginLimits::new(1, 1, BAN));
    let _first = throttle.failed(IpAddr::V4(Ipv4Addr::from(a)), now);
    let banned = throttle.failed(IpAddr::V4(Ipv4Addr::from(b)), now);
    assert!(!banned, "two sources");
    assert!(throttle.admits(IpAddr::V4(Ipv4Addr::from(b)), now));
}
