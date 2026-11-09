/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr};
use std::time::{Duration, Instant};

use crate::types::login_limits::LoginLimits;

/// The first throttled attempt's delay.
const FIRST_DELAY: Duration = Duration::from_millis(100);

/// The most a throttled attempt waits.
const MOST_DELAY: Duration = Duration::from_secs(30);

/// The longest ban the clock is asked to hold: a ban set longer lasts this long, a century.
const LONGEST_BAN: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);

/// The sources remembered. A full table forgets sources counting failures before any ban still running, so a prober cannot
/// lift a ban by spending addresses; with only bans left, a new source goes uncounted.
const SOURCES_MAX: usize = 65_536;

/// One source's failures in a row, and when its ban ends, once it has one.
#[derive(Debug, Clone, Copy, Default)]
struct Failures {
    count: u32,
    banned_until: Option<Instant>,
}

/// The frontend's failed authentications by source, held by its owner for every endpoint core (design D15). A source is an IPv4
/// address or an IPv6 /64, as one attacker holds a whole /64. Its first failures wait nothing, the next each wait a delay doubling
/// from 100 ms to at most 30 s, and then it is banned until its ban ends on its own; a success forgets its failures. The owner runs
/// on one thread, so the table is plain.
#[derive(Debug, Clone)]
pub struct AuthThrottle {
    sources: HashMap<IpAddr, Failures>,
    limits: LoginLimits,
}

impl AuthThrottle {
    /// A throttle of `limits`, remembering no source.
    #[must_use]
    pub fn new(limits: LoginLimits) -> Self {
        Self {
            sources: HashMap::new(),
            limits,
        }
    }

    /// The source `address` counts under: an IPv4 address, an IPv4-mapped IPv6 address as its IPv4 one, or an IPv6 /64.
    #[must_use]
    pub fn source_of(address: IpAddr) -> IpAddr {
        match address {
            IpAddr::V4(_) => address,
            IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
                Some(v4) => IpAddr::V4(v4),
                None => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & !u128::from(u64::MAX))),
            },
        }
    }

    /// How long an attempt waits after `throttled` failures past the throttle's threshold: none at 0, 100 ms at the first,
    /// doubling to at most 30 s.
    #[must_use]
    pub fn delay_after(throttled: u32) -> Duration {
        match throttled.checked_sub(1) {
            None => Duration::ZERO,
            Some(doublings) => FIRST_DELAY
                .checked_mul(1_u32.checked_shl(doublings).unwrap_or(u32::MAX))
                .map_or(MOST_DELAY, |delay| delay.min(MOST_DELAY)),
        }
    }

    /// Whether `address`'s source may connect at `now`: not while banned. A ban that has ended is forgotten with its failures.
    pub fn admits(&mut self, address: IpAddr, now: Instant) -> bool {
        let source = Self::source_of(address);
        match self.sources.get(&source).and_then(|held| held.banned_until) {
            Some(until) if now < until => false,
            Some(_) => {
                let _forgotten = self.sources.remove(&source);
                true
            }
            None => true,
        }
    }

    /// What `address`'s next attempt waits: nothing within the throttle's threshold, then the delay of its failures past it.
    #[must_use]
    pub fn delay(&self, address: IpAddr) -> Duration {
        let failures = self
            .sources
            .get(&Self::source_of(address))
            .map_or(0, |held| held.count);
        Self::delay_after(failures.saturating_sub(self.limits.before_throttle()))
    }

    /// `address` failed once more at `now`; whether its source is banned from here on. A failure during a ban does not extend it.
    pub fn failed(&mut self, address: IpAddr, now: Instant) -> bool {
        let source = Self::source_of(address);
        if self.sources.len() >= SOURCES_MAX && !self.sources.contains_key(&source) {
            self.sources
                .retain(|_, held| held.banned_until.is_some_and(|until| now < until));
            if self.sources.len() >= SOURCES_MAX {
                return false;
            }
        }
        let limit = self
            .limits
            .before_throttle()
            .saturating_add(self.limits.throttled_before_ban());
        let ban = self.limits.ban();
        let held = self.sources.entry(source).or_default();
        if held.banned_until.is_some() {
            return false;
        }
        held.count = held.count.saturating_add(1);
        if held.count < limit {
            return false;
        }
        held.banned_until = now.checked_add(ban.min(LONGEST_BAN)).or(Some(now));
        held.banned_until.is_some_and(|until| now < until)
    }

    /// `address` authenticated: its source's failures are forgotten.
    pub fn succeeded(&mut self, address: IpAddr) {
        let _forgotten = self.sources.remove(&Self::source_of(address));
    }
}
