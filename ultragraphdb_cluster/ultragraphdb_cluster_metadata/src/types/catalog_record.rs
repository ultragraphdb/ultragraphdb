/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, Tenant};

use crate::types::catalog_check::CatalogCheck;

/// One version of a tenant's catalog as the metadata group holds it: its tenant, its number, counted from 1 in
/// the tenant, the epoch it takes effect at once the sequencer has announced it, `None` while it is pending, where it
/// stands against the partitions' checks, and its bytes in `ultragraphdb_core_catalog`'s encoding, which the group does
/// not read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogRecord {
    tenant: Tenant,
    version: u64,
    effective: Option<Epoch>,
    check: CatalogCheck,
    bytes: Vec<u8>,
}

impl CatalogRecord {
    /// Version `version` of `tenant`'s catalog, taking effect at `effective` once announced, holding `bytes`.
    #[must_use]
    pub fn new(tenant: Tenant, version: u64, effective: Option<Epoch>, bytes: Vec<u8>) -> Self {
        Self {
            tenant,
            version,
            effective,
            check: CatalogCheck::Settled,
            bytes,
        }
    }

    /// The record standing at `check` against the partitions' checks.
    #[must_use]
    pub fn with_check(mut self, check: CatalogCheck) -> Self {
        self.check = check;
        self
    }

    /// The tenant whose catalog it is.
    #[must_use]
    pub fn tenant(&self) -> Tenant {
        self.tenant
    }

    /// The version's number.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The epoch the version takes effect at, `None` while it is pending.
    #[must_use]
    pub fn effective(&self) -> Option<Epoch> {
        self.effective
    }

    /// Where the version stands against the partitions' checks.
    #[must_use]
    pub fn check(&self) -> &CatalogCheck {
        &self.check
    }

    /// The version's bytes, in `ultragraphdb_core_catalog`'s encoding.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Assigns the epoch the version takes effect at.
    pub(crate) fn take_effect(&mut self, epoch: Epoch) {
        self.effective = Some(epoch);
    }

    /// Sets where the version stands against the partitions' checks.
    pub(crate) fn set_check(&mut self, check: CatalogCheck) {
        self.check = check;
    }
}
