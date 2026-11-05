/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A default-entry record: the default declaration of a predicate no catalog version of its tenant declares, which a
/// partition asks the metadata group to record after the predicate's first write, keyed by (tenant, predicate). It
/// is no catalog version: recording it finds a record that exists, and stales nothing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DefaultEntry {
    tenant: u64,
    predicate: String,
    entry: Vec<u8>,
}

impl DefaultEntry {
    /// The default entry `entry`, as the catalog's encoding wrote it, of the predicate named `predicate` in
    /// `tenant`.
    #[must_use]
    pub fn new(tenant: u64, predicate: String, entry: Vec<u8>) -> Self {
        Self {
            tenant,
            predicate,
            entry,
        }
    }

    /// The tenant of the predicate.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The predicate's IRI.
    #[must_use]
    pub fn predicate(&self) -> &str {
        &self.predicate
    }

    /// The default declaration, as the catalog's encoding wrote it.
    #[must_use]
    pub fn entry(&self) -> &[u8] {
        &self.entry
    }
}
