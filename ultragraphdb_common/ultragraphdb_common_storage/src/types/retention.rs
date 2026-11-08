/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use ultragraphdb_core_ids::{Epoch, Version};

/// The most often a partition asks for a higher discard version, in microseconds: each ask is a synced write.
const RAISE_EVERY_US: u64 = 1_000_000;

/// What a partition retains (`PROTOCOL.md` §11.9): each closed epoch and when it closed, and from those the version bound of an
/// epoch closed more than `read_retain_ms` ago, the highest its discard version may rise to. Time is what its caller tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retention {
    retain_us: u64,
    raise_every_us: u64,
    next_raise_us: u64,
    latest: u64,
    /// The epochs closed within the retention and one raise's interval, each newer than the last, which an epoch's length
    /// bounds.
    closes: VecDeque<(Epoch, u64)>,
}

impl Retention {
    /// Retention of `retain_ms` milliseconds: asked no more often than that, nor than once a second.
    #[must_use]
    pub fn new(retain_ms: u64) -> Self {
        let retain_us = retain_ms.saturating_mul(1_000);
        Self {
            retain_us,
            raise_every_us: retain_us.min(RAISE_EVERY_US),
            next_raise_us: 0,
            latest: 0,
            closes: VecDeque::new(),
        }
    }

    /// Notes the partition's latest closed epoch as of `now_us`; an epoch it noted already changes nothing.
    pub fn closed(&mut self, epoch: Epoch, now_us: u64) {
        if epoch.get() > self.latest {
            self.latest = epoch.get();
            self.closes.push_back((epoch, now_us));
        }
    }

    /// The version bound to raise the discard version to at `now_us`: that of the latest epoch closed more than the retention
    /// ago, when one is new since the last answer and a raise is due.
    pub fn due(&mut self, now_us: u64) -> Option<u64> {
        if now_us < self.next_raise_us {
            return None;
        }
        let mut past = None;
        while let Some((epoch, at_us)) = self.closes.front().copied()
            && now_us.saturating_sub(at_us) > self.retain_us
        {
            past = Some(epoch);
            let _taken = self.closes.pop_front();
        }
        let epoch = past?;
        self.next_raise_us = now_us.saturating_add(self.raise_every_us);
        Some(Version::bound_of(epoch))
    }
}
