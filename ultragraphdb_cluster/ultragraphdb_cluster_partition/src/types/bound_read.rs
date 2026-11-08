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
    tenant: u64,
}

impl BoundRead {
    /// The transaction bound as `key`, reading at `read_version` for `principal` in `tenant`.
    pub(crate) fn new(key: BoundKey, read_version: u64, principal: u64, tenant: u64) -> Self {
        Self {
            key,
            read_version,
            principal,
            tenant,
        }
    }

    /// The connection and stream that bound it.
    #[must_use]
    pub fn key(&self) -> BoundKey {
        self.key
    }

    /// The version it reads at.
    #[must_use]
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The principal it acts for.
    #[must_use]
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The tenant it reads and writes.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }
}
