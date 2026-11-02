/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Namespace;

/// A catalog version the metadata group holds pending: its namespace and its number. A close announces it, assigning
/// it the epoch after the close as the epoch it takes effect at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogVersion {
    namespace: Namespace,
    version: u64,
}

impl CatalogVersion {
    /// Version `version` of `namespace`'s catalog.
    #[must_use]
    pub fn new(namespace: Namespace, version: u64) -> Self {
        Self { namespace, version }
    }

    /// The namespace whose catalog it is.
    #[must_use]
    pub fn namespace(self) -> Namespace {
        self.namespace
    }

    /// The version's number.
    #[must_use]
    pub fn version(self) -> u64 {
        self.version
    }
}
