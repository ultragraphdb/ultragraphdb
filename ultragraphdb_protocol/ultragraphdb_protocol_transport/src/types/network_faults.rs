/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The faults of a [`crate::SimNetwork`], each a rate per million messages: a message dropped, delivered twice,
/// delayed by up to `max_delay` steps, or split into several reads. Delays keep each link's order, as a TCP stream
/// does: a delayed message holds back those after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NetworkFaults {
    drop: u32,
    duplicate: u32,
    delay: u32,
    split: u32,
    max_delay: u64,
}

impl NetworkFaults {
    /// No faults.
    pub fn none() -> Self {
        Self::default()
    }

    pub fn with_drop(mut self, per_million: u32) -> Self {
        self.drop = per_million;
        self
    }

    pub fn with_duplicate(mut self, per_million: u32) -> Self {
        self.duplicate = per_million;
        self
    }

    /// Delays at `per_million`, each by 1 to `max_delay` steps.
    pub fn with_delay(mut self, per_million: u32, max_delay: u64) -> Self {
        self.delay = per_million;
        self.max_delay = max_delay;
        self
    }

    pub fn with_split(mut self, per_million: u32) -> Self {
        self.split = per_million;
        self
    }

    pub fn drop_rate(&self) -> u32 {
        self.drop
    }

    pub fn duplicate_rate(&self) -> u32 {
        self.duplicate
    }

    pub fn delay_rate(&self) -> u32 {
        self.delay
    }

    pub fn split_rate(&self) -> u32 {
        self.split
    }

    pub fn max_delay(&self) -> u64 {
        self.max_delay
    }
}
