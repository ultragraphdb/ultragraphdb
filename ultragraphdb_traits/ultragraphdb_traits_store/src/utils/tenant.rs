/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Where a key's tenant begins: Dgraph's `x.NamespaceOffset`, after the key's prefix byte.
pub const TENANT_OFFSET: usize = 1;

/// The tenant a ban applies to `key`: the eight big-endian bytes at [`TENANT_OFFSET`], when the key is longer
/// than them, as Badger's `isBanned` reads it (`db.go:1921-1932`); `None` for a shorter key, which no ban covers.
pub fn key_tenant(key: &[u8]) -> Option<u64> {
    let bytes = key.get(TENANT_OFFSET..TENANT_OFFSET + 8)?;
    (key.len() > TENANT_OFFSET + 8).then(|| {
        bytes
            .iter()
            .fold(0u64, |tenant, &b| tenant << 8 | u64::from(b))
    })
}
