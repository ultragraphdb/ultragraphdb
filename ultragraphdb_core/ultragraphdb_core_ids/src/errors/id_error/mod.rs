/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod id_error_display;
mod id_error_error;

/// Why a value of `ultragraphdb_core_ids` was refused.
///
/// - **Invalid system ID:** [`ReservedBits`](IdErrorEnum::ReservedBits),
///   [`ZeroTypePrefix`](IdErrorEnum::ZeroTypePrefix), [`ZeroSequence`](IdErrorEnum::ZeroSequence). Decoded bytes or
///   numbers that name no entity; the caller refuses what carried them.
/// - **Out of range:** [`Length`](IdErrorEnum::Length), [`BucketRange`](IdErrorEnum::BucketRange),
///   [`BucketGroupRange`](IdErrorEnum::BucketGroupRange),
///   [`IriTooLong`](IdErrorEnum::IriTooLong), [`EpochRange`](IdErrorEnum::EpochRange),
///   [`CommitSequenceRange`](IdErrorEnum::CommitSequenceRange),
///   [`OddCommitSequence`](IdErrorEnum::OddCommitSequence). A value outside what its field holds.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdError(pub IdErrorEnum);

/// The classification of an [`IdError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdErrorEnum {
    /// `what` is `expected` bytes long, and `found` bytes were given.
    Length {
        /// What was decoded: "a system ID", "a version".
        what: &'static str,
        /// The length it has.
        expected: usize,
        /// The length given.
        found: usize,
    },
    /// An ID's 24 reserved bits are `bits`, not zero.
    ReservedBits {
        /// The reserved bits as decoded, the low 24 bits of the ID.
        bits: u32,
    },
    /// An ID's type prefix is 0, which no type has.
    ZeroTypePrefix,
    /// An ID's sequence is 0, which no entity has.
    ZeroSequence,
    /// `bucket` does not fit in a bucket's 24 bits.
    BucketRange {
        /// The value refused.
        bucket: u32,
    },
    /// `group` does not fit in a bucket group's 18 bits.
    BucketGroupRange {
        /// The value refused.
        group: u32,
    },
    /// An IRI of `len` bytes is longer than a canonical key's 4-byte length counts.
    IriTooLong {
        /// The IRI's length in bytes.
        len: usize,
    },
    /// `epoch` does not fit in an epoch's 42 bits.
    EpochRange {
        /// The value refused, or the epoch past the last when the next was asked.
        epoch: u64,
    },
    /// A commit version's sequence `sequence` does not fit in its 22 bits as an even number.
    CommitSequenceRange {
        /// The value refused.
        sequence: u32,
    },
    /// A commit version's sequence `sequence` is odd; commit versions are even.
    OddCommitSequence {
        /// The value refused.
        sequence: u32,
    },
}

impl IdError {
    /// `what` is `expected` bytes long, and `found` bytes were given.
    #[must_use]
    pub fn length(what: &'static str, expected: usize, found: usize) -> Self {
        Self(IdErrorEnum::Length {
            what,
            expected,
            found,
        })
    }

    /// An ID's reserved bits are `bits`, not zero.
    #[must_use]
    pub fn reserved_bits(bits: u32) -> Self {
        Self(IdErrorEnum::ReservedBits { bits })
    }

    /// An ID's type prefix is 0.
    #[must_use]
    pub fn zero_type_prefix() -> Self {
        Self(IdErrorEnum::ZeroTypePrefix)
    }

    /// An ID's sequence is 0.
    #[must_use]
    pub fn zero_sequence() -> Self {
        Self(IdErrorEnum::ZeroSequence)
    }

    /// `bucket` does not fit in 24 bits.
    #[must_use]
    pub fn bucket_range(bucket: u32) -> Self {
        Self(IdErrorEnum::BucketRange { bucket })
    }

    /// `group` does not fit in 18 bits.
    #[must_use]
    pub fn bucket_group_range(group: u32) -> Self {
        Self(IdErrorEnum::BucketGroupRange { group })
    }

    /// An IRI of `len` bytes is too long for a canonical key.
    #[must_use]
    pub fn iri_too_long(len: usize) -> Self {
        Self(IdErrorEnum::IriTooLong { len })
    }

    /// `epoch` does not fit in 42 bits.
    #[must_use]
    pub fn epoch_range(epoch: u64) -> Self {
        Self(IdErrorEnum::EpochRange { epoch })
    }

    /// A commit sequence `sequence` does not fit in 22 bits.
    #[must_use]
    pub fn commit_sequence_range(sequence: u32) -> Self {
        Self(IdErrorEnum::CommitSequenceRange { sequence })
    }

    /// A commit sequence `sequence` is odd.
    #[must_use]
    pub fn odd_commit_sequence(sequence: u32) -> Self {
        Self(IdErrorEnum::OddCommitSequence { sequence })
    }
}
