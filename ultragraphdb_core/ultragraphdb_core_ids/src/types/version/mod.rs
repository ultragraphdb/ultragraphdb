/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::id_error::IdError;

use crate::types::epoch::Epoch;

/// The bits of the sequence within an epoch, below the epoch's 42.
const SEQUENCE_BITS: u32 = 22;

/// The sequence's bits within the top byte of the 3 low bytes: 22 = 6 + 8 + 8.
const SEQUENCE_TOP_MASK: u8 = 0x3F;

/// A version, 64 bits: a 42-bit [`Epoch`] in the upper bits and a 22-bit sequence within it in the lower. A commit
/// version's sequence is even ([`Version::commit`]); a rollup writes below the deltas it folds, at any free version,
/// so a version as stored and decoded may be odd. Versions order as unsigned numbers: by epoch, then sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(u64);

impl Version {
    /// The length of an encoded version in bytes.
    pub const BYTES: usize = 8;
    /// The largest sequence of a commit version, 2^22 − 2, the largest even number in 22 bits.
    pub const COMMIT_SEQUENCE_MAX: u32 = (1 << 22) - 2;

    /// The commit version of `epoch` and `sequence`: `epoch << 22 | sequence`.
    ///
    /// # Errors
    ///
    /// [`IdError::odd_commit_sequence`] when `sequence` is odd, then [`IdError::commit_sequence_range`] when it
    /// is above [`Version::COMMIT_SEQUENCE_MAX`].
    pub fn commit(epoch: Epoch, sequence: u32) -> Result<Self, IdError> {
        if !sequence.is_multiple_of(2) {
            return Err(IdError::odd_commit_sequence(sequence));
        }
        if sequence > Self::COMMIT_SEQUENCE_MAX {
            return Err(IdError::commit_sequence_range(sequence));
        }
        // An epoch holds 42 bits, so shifting it by 22 keeps every bit, and the sequence fills the 22 freed.
        Ok(Self((epoch.get() << SEQUENCE_BITS) | u64::from(sequence)))
    }

    /// The version whose number is `value`. Every 64-bit number is a version.
    #[must_use]
    pub fn from_u64(value: u64) -> Self {
        Self(value)
    }

    /// The version encoded in `bytes`, its 8 big-endian bytes.
    ///
    /// # Errors
    ///
    /// [`IdError::length`] when `bytes` is not 8 bytes long.
    pub fn decode(bytes: &[u8]) -> Result<Self, IdError> {
        let array = <[u8; Self::BYTES]>::try_from(bytes)
            .map_err(|_| IdError::length("a version", Self::BYTES, bytes.len()))?;
        Ok(Self(u64::from_be_bytes(array)))
    }

    /// The version as a number.
    #[must_use]
    pub fn to_u64(self) -> u64 {
        self.0
    }

    /// The version as its 8 big-endian bytes.
    #[must_use]
    pub fn to_be_bytes(self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    /// The epoch, the upper 42 bits.
    #[must_use]
    pub fn epoch(self) -> Epoch {
        Epoch::from_low_bits(self.0 >> SEQUENCE_BITS)
    }

    /// The sequence within the epoch, the lower 22 bits.
    #[must_use]
    pub fn sequence(self) -> u32 {
        let [.., top, middle, low] = self.0.to_be_bytes();
        u32::from_be_bytes([0, top & SEQUENCE_TOP_MASK, middle, low])
    }

    /// Whether this is a commit version, its sequence even.
    #[must_use]
    pub fn is_commit(self) -> bool {
        self.0.is_multiple_of(2)
    }
}
