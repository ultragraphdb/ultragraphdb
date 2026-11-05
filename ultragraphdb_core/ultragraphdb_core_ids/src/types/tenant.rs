/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use xxhash_rust::xxh3::xxh3_64;

/// A tenant, the unit of isolation. Its 8 big-endian bytes are hashed into the top 8 bits of every
/// bucket of its entities ([`Bucket::derive`](crate::Bucket::derive)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tenant(u64);

impl Tenant {
    /// The tenant `tenant`.
    #[must_use]
    pub fn new(tenant: u64) -> Self {
        Self(tenant)
    }

    /// The tenant as a number.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    /// The top 8 bits of every bucket of the tenant's entities: the top byte of XXH3-64 (seed 0) of the
    /// tenant's 8 big-endian bytes. The placement map names the tenant's buckets, its run, by them.
    #[must_use]
    pub fn bucket_bits(self) -> u8 {
        let [top, ..] = xxh3_64(&self.0.to_be_bytes()).to_be_bytes();
        top
    }
}
