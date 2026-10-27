/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a [`crate::SimNetwork`] did to the messages written to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NetworkStats {
    pub(crate) sent: u64,
    pub(crate) dropped: u64,
    pub(crate) duplicated: u64,
    pub(crate) delayed: u64,
    pub(crate) split: u64,
}

impl NetworkStats {
    /// The messages written.
    pub fn sent(&self) -> u64 {
        self.sent
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn duplicated(&self) -> u64 {
        self.duplicated
    }

    pub fn delayed(&self) -> u64 {
        self.delayed
    }

    pub fn split(&self) -> u64 {
        self.split
    }
}
