/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What the owner says of a tenant and the sequencer when a request names one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantFacts {
    known: bool,
    open_epoch: u64,
    last_closed: u64,
    current_version: u64,
}

impl TenantFacts {
    /// Whether the group holds the tenant, the sequencer's open and last closed epochs, and the tenant's latest catalog version.
    #[must_use]
    pub fn new(known: bool, open_epoch: u64, last_closed: u64, current_version: u64) -> Self {
        Self {
            known,
            open_epoch,
            last_closed,
            current_version,
        }
    }

    /// Whether the group holds the tenant.
    #[must_use]
    pub fn known(&self) -> bool {
        self.known
    }

    /// The sequencer's open epoch.
    #[must_use]
    pub fn open_epoch(&self) -> u64 {
        self.open_epoch
    }

    /// The latest epoch the sequencer closed.
    #[must_use]
    pub fn last_closed(&self) -> u64 {
        self.last_closed
    }

    /// The tenant's latest catalog version, pending or not.
    #[must_use]
    pub fn current_version(&self) -> u64 {
        self.current_version
    }
}
