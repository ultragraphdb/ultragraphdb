/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A catalog version the metadata group holds: its tenant, its number, the epoch it takes effect at, and its
/// contents, the bytes the catalog's encoding wrote, which the group treats as opaque.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeldVersion {
    tenant: u64,
    version: u64,
    effective: u64,
    contents: Vec<u8>,
}

impl HeldVersion {
    /// Version `version` of `tenant`'s catalog, in effect from epoch `effective`, holding `contents`.
    #[must_use]
    pub fn new(tenant: u64, version: u64, effective: u64, contents: Vec<u8>) -> Self {
        Self {
            tenant,
            version,
            effective,
            contents,
        }
    }

    /// The tenant whose catalog it is.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The version's number.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The epoch the version takes effect at.
    #[must_use]
    pub fn effective(&self) -> u64 {
        self.effective
    }

    /// The version's contents, as the catalog's encoding wrote them.
    #[must_use]
    pub fn contents(&self) -> &[u8] {
        &self.contents
    }
}
