/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A rebuild under way, as the store keeps it until a batch removes it: its number and the prefixes of the indexes
/// the new schema adds, which writes maintain while it builds. [`crate::Store::write_rebuild_record`] makes it
/// durable before the build starts; the swap batch or the failure batch removes it
/// ([`crate::WriteBatch::remove_rebuild_record`]). A record found when the store is opened names a rebuild a crash or
/// a power loss stopped, and the caller drops what it names that the schema it loads does not declare.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RebuildRecord {
    id: u64,
    prefixes: Vec<Vec<u8>>,
}

impl RebuildRecord {
    pub fn new(id: u64, prefixes: Vec<Vec<u8>>) -> Self {
        Self { id, prefixes }
    }

    /// The rebuild's number, unique among the records the store holds.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn prefixes(&self) -> &[Vec<u8>] {
        &self.prefixes
    }

    pub fn into_prefixes(self) -> Vec<Vec<u8>> {
        self.prefixes
    }

    /// Whether `key` lies under one of the record's prefixes.
    pub fn covers(&self, key: &[u8]) -> bool {
        self.prefixes.iter().any(|p| key.starts_with(p))
    }
}
