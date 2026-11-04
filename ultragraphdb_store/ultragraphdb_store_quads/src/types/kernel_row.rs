/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;

/// One row of the read kernel ([`crate::QuadStore::scan`]): a list's key, without its version, and the list read at
/// the scan's version, each object with the statement IDs it is held under.
#[derive(Debug, Clone, PartialEq)]
pub struct KernelRow {
    key: Vec<u8>,
    objects: Vec<(EncodedTerm, Vec<SystemId>)>,
}

impl KernelRow {
    /// The row of the list `key` holding `objects`.
    pub fn new(key: Vec<u8>, objects: Vec<(EncodedTerm, Vec<SystemId>)>) -> Self {
        Self { key, objects }
    }

    /// The list's key, without its version.
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// The list's objects at the scan's version.
    pub fn objects(&self) -> &[(EncodedTerm, Vec<SystemId>)] {
        &self.objects
    }
}
