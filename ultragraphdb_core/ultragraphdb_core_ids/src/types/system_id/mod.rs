/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::id_error::IdError;

use crate::types::bucket::Bucket;
use crate::types::canonical_key::CanonicalKey;
use crate::types::sequence::Sequence;
use crate::types::tenant::Tenant;
use crate::types::type_prefix::TypePrefix;

/// The 128-bit system ID that names an entity or a statement inside the database, never outside it. From the most
/// significant bit: a 16-bit type prefix (bits 127–112), a 64-bit sequence (111–48), a 24-bit bucket (47–24) and 24
/// reserved bits (23–0), which are zero. It is stored as its 16 big-endian bytes and ordered as that unsigned value:
/// by type, then sequence, then bucket. The bucket's hashed bits sit below the sequence, so the IDs of a sorted list
/// differ mostly in their sequences and their deltas stay small. A zero prefix, a zero sequence and nonzero reserved
/// bits are not representable; decoding refuses them.
// The fields are declared in the order of their bytes, so the derived order is the order of the 16 big-endian bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemId {
    prefix: TypePrefix,
    sequence: Sequence,
    bucket: Bucket,
}

impl SystemId {
    /// The length of an encoded ID in bytes.
    pub const BYTES: usize = 16;

    /// The ID of `prefix`, `bucket` and `sequence`, its reserved bits zero.
    #[must_use]
    pub fn new(prefix: TypePrefix, bucket: Bucket, sequence: Sequence) -> Self {
        Self {
            prefix,
            sequence,
            bucket,
        }
    }

    /// The ID of the `Resource` whose key is `key` in `tenant`, numbered `sequence`: type prefix 1 and the
    /// bucket [`Bucket::derive`] gives.
    #[must_use]
    pub fn resource(tenant: Tenant, key: &CanonicalKey<'_>, sequence: Sequence) -> Self {
        let prefix = TypePrefix::RESOURCE;
        Self::new(prefix, Bucket::derive(tenant, prefix, key), sequence)
    }

    /// The ID of a statement whose subject is `subject`, numbered `sequence`: type prefix 2 and the subject's
    /// bucket.
    #[must_use]
    pub fn statement(subject: Self, sequence: Sequence) -> Self {
        Self::new(TypePrefix::STATEMENT, subject.bucket, sequence)
    }

    /// The type prefix.
    #[must_use]
    pub fn type_prefix(self) -> TypePrefix {
        self.prefix
    }

    /// The bucket.
    #[must_use]
    pub fn bucket(self) -> Bucket {
        self.bucket
    }

    /// The sequence.
    #[must_use]
    pub fn sequence(self) -> Sequence {
        self.sequence
    }

    /// The ID as an unsigned 128-bit value.
    #[must_use]
    pub fn to_u128(self) -> u128 {
        u128::from_be_bytes(self.to_be_bytes())
    }

    /// The ID as its 16 big-endian bytes.
    #[must_use]
    pub fn to_be_bytes(self) -> [u8; 16] {
        let [t0, t1] = self.prefix.get().to_be_bytes();
        let [s0, s1, s2, s3, s4, s5, s6, s7] = self.sequence.get().to_be_bytes();
        let [b_top, b0, b1, b2] = self.bucket.get().to_be_bytes();
        debug_assert_eq!(b_top, 0, "a bucket holds 24 bits");
        // The reserved bits are zero.
        [t0, t1, s0, s1, s2, s3, s4, s5, s6, s7, b0, b1, b2, 0, 0, 0]
    }

    /// The ID whose unsigned 128-bit value is `value`.
    ///
    /// # Errors
    ///
    /// Checked in this order: [`IdError::reserved_bits`] naming the low 24 bits when they are not zero,
    /// [`IdError::zero_type_prefix`] when the top 16 bits are zero, [`IdError::zero_sequence`] when the sequence's
    /// 64 bits are zero.
    pub fn from_u128(value: u128) -> Result<Self, IdError> {
        let [
            t0,
            t1,
            s0,
            s1,
            s2,
            s3,
            s4,
            s5,
            s6,
            s7,
            b0,
            b1,
            b2,
            r0,
            r1,
            r2,
        ] = value.to_be_bytes();
        let reserved = u32::from_be_bytes([0, r0, r1, r2]);
        if reserved != 0 {
            return Err(IdError::reserved_bits(reserved));
        }
        let prefix = TypePrefix::new(u16::from_be_bytes([t0, t1]))?;
        let sequence = Sequence::new(u64::from_be_bytes([s0, s1, s2, s3, s4, s5, s6, s7]))?;
        // Three bytes under a zero top byte: always below 2^24, so this never refuses.
        let bucket = Bucket::new(u32::from_be_bytes([0, b0, b1, b2]))?;
        Ok(Self::new(prefix, bucket, sequence))
    }

    /// The ID encoded in `bytes`, its 16 big-endian bytes.
    ///
    /// # Errors
    ///
    /// [`IdError::length`] when `bytes` is not 16 bytes long; otherwise the refusals of [`SystemId::from_u128`].
    pub fn decode(bytes: &[u8]) -> Result<Self, IdError> {
        let array = <[u8; Self::BYTES]>::try_from(bytes)
            .map_err(|_| IdError::length("a system ID", Self::BYTES, bytes.len()))?;
        Self::from_u128(u128::from_be_bytes(array))
    }
}
