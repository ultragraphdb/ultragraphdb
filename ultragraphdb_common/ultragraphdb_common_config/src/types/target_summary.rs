/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What `GET /v1/targets` lists of one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSummary {
    pub(crate) name: String,
    pub(crate) latest: u64,
    pub(crate) active: Option<u64>,
    pub(crate) retired: bool,
}

impl TargetSummary {
    /// The target's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its latest revision.
    #[must_use]
    pub fn latest(&self) -> u64 {
        self.latest
    }

    /// Its active revision.
    #[must_use]
    pub fn active(&self) -> Option<u64> {
        self.active
    }

    /// Whether it is retired.
    #[must_use]
    pub fn retired(&self) -> bool {
        self.retired
    }
}
