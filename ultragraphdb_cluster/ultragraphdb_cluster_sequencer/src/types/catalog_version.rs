/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Tenant;

/// A catalog version the metadata group holds pending: its tenant and its number. A close announces it, assigning
/// it the epoch after the close as the epoch it takes effect at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogVersion {
    tenant: Tenant,
    version: u64,
}

impl CatalogVersion {
    /// Version `version` of `tenant`'s catalog.
    #[must_use]
    pub fn new(tenant: Tenant, version: u64) -> Self {
        Self { tenant, version }
    }

    /// The tenant whose catalog it is.
    #[must_use]
    pub fn tenant(self) -> Tenant {
        self.tenant
    }

    /// The version's number.
    #[must_use]
    pub fn version(self) -> u64 {
        self.version
    }
}
