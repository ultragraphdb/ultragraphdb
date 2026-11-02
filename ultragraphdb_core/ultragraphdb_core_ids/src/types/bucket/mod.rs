/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::id_error::IdError;
use xxhash_rust::xxh3::Xxh3Default;

use crate::types::canonical_key::CanonicalKey;
use crate::types::namespace::Namespace;
use crate::types::type_prefix::TypePrefix;

/// The 24-bit bucket of a [`SystemId`](crate::SystemId), which decides the partition an entity lives in. It is
/// fixed when the entity is created and computed, never chosen ([`Bucket::derive`]); a statement takes its
/// subject's bucket, and a blank node the bucket its skolem IRI states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bucket(u32);

impl Bucket {
    /// The largest bucket, 2^24 − 1.
    pub const MAX: Self = Self((1 << 24) - 1);

    /// The bucket `bucket`.
    ///
    /// # Errors
    ///
    /// [`IdError::bucket_range`] when `bucket` is above [`Bucket::MAX`].
    pub fn new(bucket: u32) -> Result<Self, IdError> {
        if bucket <= Self::MAX.0 {
            Ok(Self(bucket))
        } else {
            Err(IdError::bucket_range(bucket))
        }
    }

    /// The bucket as a number below 2^24.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }

    /// The bucket of the entity of the type declaring `prefix` whose key is `key`, in `namespace`: its top 8
    /// bits are the top 8 bits of XXH3-64 (seed 0) of the namespace's 8 big-endian bytes, its low 16 bits the top 16
    /// bits of XXH3-64 (seed 0) of the prefix's 2 big-endian bytes followed by the key's canonical encoding. The same
    /// namespace, type and key give the same bucket on every node and in every version.
    #[must_use]
    pub fn derive(namespace: Namespace, prefix: TypePrefix, key: &CanonicalKey<'_>) -> Self {
        let namespace_top = namespace.bucket_bits();
        // Streamed, so no buffer holds the prefix and the key together; XXH3's streaming digest equals its one-shot
        // hash of the concatenated bytes.
        let mut hasher = Xxh3Default::new();
        hasher.update(&prefix.get().to_be_bytes());
        hasher.update(&key.length_bytes());
        hasher.update(key.as_str().as_bytes());
        let [key_high, key_low, ..] = hasher.digest().to_be_bytes();
        // The top byte is zero, so the bucket holds 24 bits by construction.
        let bucket = u32::from_be_bytes([0, namespace_top, key_high, key_low]);
        debug_assert!(bucket <= Self::MAX.0, "a derived bucket holds 24 bits");
        Self(bucket)
    }
}
