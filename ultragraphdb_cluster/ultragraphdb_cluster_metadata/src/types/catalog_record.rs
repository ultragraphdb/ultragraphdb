/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, Namespace};

/// One version of a namespace's catalog as the metadata group holds it: its namespace, its number, counted from 1 in
/// the namespace, the epoch it takes effect at once the sequencer has announced it, `None` while it is pending, and its
/// bytes in `ultragraphdb_core_catalog`'s encoding, which the group does not read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogRecord {
    namespace: Namespace,
    version: u64,
    effective: Option<Epoch>,
    bytes: Vec<u8>,
}

impl CatalogRecord {
    /// Version `version` of `namespace`'s catalog, taking effect at `effective` once announced, holding `bytes`.
    #[must_use]
    pub fn new(
        namespace: Namespace,
        version: u64,
        effective: Option<Epoch>,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            namespace,
            version,
            effective,
            bytes,
        }
    }

    /// The namespace whose catalog it is.
    #[must_use]
    pub fn namespace(&self) -> Namespace {
        self.namespace
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

    /// The version's bytes, in `ultragraphdb_core_catalog`'s encoding.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Assigns the epoch the version takes effect at.
    pub(crate) fn take_effect(&mut self, epoch: Epoch) {
        self.effective = Some(epoch);
    }
}
