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

    /// Drops at `per_million`.
    pub fn with_drop(mut self, per_million: u32) -> Self {
        self.drop = per_million;
        self
    }

    /// Duplicates at `per_million`.
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

    /// Splits at `per_million`.
    pub fn with_split(mut self, per_million: u32) -> Self {
        self.split = per_million;
        self
    }

    /// The rate of drops, per million messages.
    pub fn drop_rate(&self) -> u32 {
        self.drop
    }

    /// The rate of duplicates, per million messages.
    pub fn duplicate_rate(&self) -> u32 {
        self.duplicate
    }

    /// The rate of delays, per million messages.
    pub fn delay_rate(&self) -> u32 {
        self.delay
    }

    /// The rate of splits, per million messages.
    pub fn split_rate(&self) -> u32 {
        self.split
    }

    /// The most steps a delay holds a message back.
    pub fn max_delay(&self) -> u64 {
        self.max_delay
    }
}
