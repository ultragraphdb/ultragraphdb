/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::IpAddr;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The first failure's delay on the next attempt from its address.
const FIRST_DELAY: Duration = Duration::from_millis(100);

/// The most an address waits, however many times it failed.
const MOST_DELAY: Duration = Duration::from_secs(30);

/// The addresses remembered; past it the table starts over, which forgives failures rather than holding memory for a prober.
const ADDRESSES_MAX: usize = 65_536;

/// The names remembered; past it the table starts over, as the addresses' does.
const NAMES_MAX: usize = 65_536;

/// The consecutive failures of a name past which it is refused from every source (`SECURITY.md`).
const NAME_FAILURES_MAX: u32 = 100;

/// One name's failures in a row, and when its refusal ends, once they reached [`NAME_FAILURES_MAX`].
#[derive(Debug, Clone, Copy, Default)]
struct NameFailures {
    count: u32,
    refused_until: Option<Instant>,
}

/// Repeated failed authentications, on one core, by source address and by name: each failure doubles what the next attempt
/// waits, and a success forgets them. An address's owes nothing to the name tried, so it tells a prober nothing of it; a name's
/// count holds for a name no principal holds, so it tells nothing either. A name past [`NAME_FAILURES_MAX`] is refused from every
/// source until its interval ends on its own (`SECURITY.md`).
#[derive(Debug, Clone)]
pub struct AuthThrottle {
    failures: Rc<RefCell<HashMap<IpAddr, u32>>>,
    names: Rc<RefCell<HashMap<String, NameFailures>>>,
    interval: Duration,
}

impl AuthThrottle {
    /// A throttle refusing a name past its failures for `interval`.
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self {
            failures: Rc::default(),
            names: Rc::default(),
            interval,
        }
    }

    /// How long an attempt waits after `failures` in a row.
    #[must_use]
    pub fn delay_after(failures: u32) -> Duration {
        match failures.checked_sub(1) {
            None => Duration::ZERO,
            Some(doublings) => FIRST_DELAY
                .checked_mul(1_u32.checked_shl(doublings).unwrap_or(u32::MAX))
                .map_or(MOST_DELAY, |delay| delay.min(MOST_DELAY)),
        }
    }

    /// What `address`'s next attempt waits.
    #[must_use]
    pub fn delay(&self, address: IpAddr) -> Duration {
        let failures = self.failures.borrow().get(&address).copied().unwrap_or(0);
        Self::delay_after(failures)
    }

    /// `address` failed once more.
    pub fn failed(&self, address: IpAddr) {
        let mut failures = self.failures.borrow_mut();
        if failures.len() >= ADDRESSES_MAX && !failures.contains_key(&address) {
            failures.clear();
        }
        let count = failures.entry(address).or_insert(0);
        *count = count.saturating_add(1);
    }

    /// `address` authenticated.
    pub fn succeeded(&self, address: IpAddr) {
        let _forgotten = self.failures.borrow_mut().remove(&address);
    }

    /// What an attempt naming `name` waits: nothing while it is refused, which answers at once.
    #[must_use]
    pub fn name_delay(&self, name: &str) -> Duration {
        match self.names.borrow().get(name) {
            Some(held) if held.refused_until.is_none() => Self::delay_after(held.count),
            Some(_) | None => Duration::ZERO,
        }
    }

    /// Whether `name` is refused at `now`, from every source; a refusal whose interval ended forgets its failures.
    pub fn name_refused(&self, name: &str, now: Instant) -> bool {
        let mut names = self.names.borrow_mut();
        match names.get(name).and_then(|held| held.refused_until) {
            Some(until) if now < until => true,
            Some(_) => {
                let _forgotten = names.remove(name);
                false
            }
            None => false,
        }
    }

    /// `name` failed once more at `now`. A failure during its refusal does not extend it.
    pub fn name_failed(&self, name: &str, now: Instant) {
        let mut names = self.names.borrow_mut();
        if names.len() >= NAMES_MAX && !names.contains_key(name) {
            // Room comes from names counting failures and refusals ended, never from a refusal still running; with none to free,
            // the new name goes uncounted, its address's delay still holding it back.
            names.retain(|_, held| held.refused_until.is_some_and(|until| now < until));
            if names.len() >= NAMES_MAX {
                return;
            }
        }
        let held = names.entry(name.to_string()).or_default();
        if held.refused_until.is_some() {
            return;
        }
        held.count = held.count.saturating_add(1);
        if held.count >= NAME_FAILURES_MAX {
            held.refused_until = Some(now + self.interval);
        }
    }

    /// `name` authenticated.
    pub fn name_succeeded(&self, name: &str) {
        let _forgotten = self.names.borrow_mut().remove(name);
    }
}
