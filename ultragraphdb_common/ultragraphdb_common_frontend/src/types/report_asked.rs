/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Tenant;

/// A partition's report on a catalog version (`PROTOCOL.md` §13.19), its terms held past its frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportAsked {
    tenant: Tenant,
    version: u64,
    partition: u32,
    refused: Option<(String, String)>,
}

impl ReportAsked {
    /// `partition`'s report on `version` of `tenant`'s catalog: `refused` names the predicate and the term at fault, or
    /// is `None` for a check passed. The tenant is a [`Tenant`], so it and the version cannot be swapped.
    #[must_use]
    pub fn new(
        tenant: Tenant,
        version: u64,
        partition: u32,
        refused: Option<(String, String)>,
    ) -> Self {
        Self {
            tenant,
            version,
            partition,
            refused,
        }
    }

    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> Tenant {
        self.tenant
    }

    /// The version reported on.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The reporting partition, as the frame names it: 0 is no partition, which the owner refuses.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// What the data refused: the predicate and the term at fault; `None` for a check passed.
    #[must_use]
    pub fn refused(&self) -> Option<(&str, &str)> {
        self.refused
            .as_ref()
            .map(|(predicate, term)| (predicate.as_str(), term.as_str()))
    }

    /// What the data refused, taken out of the report, as [`ReportAsked::refused`] reads it.
    #[must_use]
    pub fn into_refused(self) -> Option<(String, String)> {
        self.refused
    }
}
