/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod catalog_contents_codec;

use ultragraphdb_core_mapping::Declaration;

/// The contents of one catalog version of a tenant, as the metadata group holds it: the tenant, the version's
/// number and every declaration the version makes, each naming its predicate, type or relation by IRI, since a
/// version is read by every partition and the frontend, which share no dictionary. Encoded, it is the opaque bytes the
/// group stores ([`CatalogContents::encode`]); a version that does not decode refuses its tenant's load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CatalogContents {
    tenant: u64,
    version: u64,
    declarations: Vec<Declaration>,
}

impl CatalogContents {
    /// Version `version` of `tenant`'s catalog, making `declarations`, in their order.
    #[must_use]
    pub fn new(tenant: u64, version: u64, declarations: Vec<Declaration>) -> Self {
        Self {
            tenant,
            version,
            declarations,
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

    /// Every declaration the version makes.
    #[must_use]
    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }
}
