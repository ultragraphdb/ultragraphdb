/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::bound_transaction::BoundKey;

/// What a Scan naming a bound transaction is checked against (`PROTOCOL.md` §13.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundRead {
    key: BoundKey,
    read_version: u64,
    principal: u64,
    namespace: u64,
}

impl BoundRead {
    /// The transaction bound as `key`, reading at `read_version` for `principal` in `namespace`.
    pub(crate) fn new(key: BoundKey, read_version: u64, principal: u64, namespace: u64) -> Self {
        Self {
            key,
            read_version,
            principal,
            namespace,
        }
    }

    /// The connection and stream that bound it.
    pub fn key(&self) -> BoundKey {
        self.key
    }

    /// The version it reads at.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The principal it acts for.
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The namespace it reads and writes.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }
}
