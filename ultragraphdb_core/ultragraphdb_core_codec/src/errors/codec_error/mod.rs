/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod codec_error_display;
mod codec_error_error;

use ultragraphdb_core_ids::IdError;

/// Why a list of system IDs could not be packed, or a pack's bytes or a block's deltas could not be read: a list out
/// of order, or bytes that are corrupt. Go's codec has no error: it packs any list, never returns on a truncated
/// varint and panics on an overlong one.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodecError(pub CodecErrorEnum);

/// The classification of a [`CodecError`]. A `position` counts IDs from 0, in the list being packed or the block being
/// read; an `offset` counts bytes from 0, in the buffer, the block's deltas or the pack's bytes being read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CodecErrorEnum {
    /// The ID at `position` is not greater than the one before it, or a delta read for it is 0.
    OutOfOrder {
        /// The position of the ID.
        position: usize,
    },
    /// The bytes end inside a varint, or before the varint a count promises.
    TruncatedVarint {
        /// The byte at which the varint starts.
        offset: usize,
    },
    /// A varint is longer than its width allows, or its value or a running sum exceeds 128 bits.
    OverflowingVarint {
        /// The byte at which the varint starts.
        offset: usize,
    },
    /// A varint's last group is a redundant zero, so the value has a shorter encoding.
    NonCanonicalVarint {
        /// The byte at which the varint starts.
        offset: usize,
    },
    /// The delta read for the ID at `position` carries it past 104 bits.
    DeltaOverflow {
        /// The position of the ID.
        position: usize,
    },
    /// The ID at `position` is not a valid system ID.
    InvalidId {
        /// The position of the ID.
        position: usize,
        /// Why the ID is not valid.
        error: IdError,
    },
    /// Bytes follow a block's last delta.
    TrailingDeltas {
        /// The byte of the deltas at which they start.
        offset: usize,
    },
    /// A pack's bytes end inside a field.
    Truncated {
        /// The byte at which the field starts.
        offset: usize,
    },
    /// A pack's bytes begin with a version this code does not read.
    UnknownVersion {
        /// The version byte.
        version: u8,
    },
    /// Bytes follow a pack's last block.
    TrailingBytes {
        /// The byte at which they start.
        offset: usize,
    },
    /// A block's first ID is not above the previous block's last ID, or its first when that block is empty.
    BlocksOutOfOrder {
        /// The index of the block.
        block: usize,
    },
}

impl CodecError {
    /// A [`CodecErrorEnum::OutOfOrder`] for the ID at `position`.
    #[must_use]
    pub fn out_of_order(position: usize) -> Self {
        Self(CodecErrorEnum::OutOfOrder { position })
    }

    /// A [`CodecErrorEnum::TruncatedVarint`] for the varint starting at `offset`.
    #[must_use]
    pub fn truncated_varint(offset: usize) -> Self {
        Self(CodecErrorEnum::TruncatedVarint { offset })
    }

    /// A [`CodecErrorEnum::OverflowingVarint`] for the varint starting at `offset`.
    #[must_use]
    pub fn overflowing_varint(offset: usize) -> Self {
        Self(CodecErrorEnum::OverflowingVarint { offset })
    }

    /// A [`CodecErrorEnum::NonCanonicalVarint`] for the varint starting at `offset`.
    #[must_use]
    pub fn non_canonical_varint(offset: usize) -> Self {
        Self(CodecErrorEnum::NonCanonicalVarint { offset })
    }

    /// A [`CodecErrorEnum::DeltaOverflow`] for the ID at `position`.
    #[must_use]
    pub fn delta_overflow(position: usize) -> Self {
        Self(CodecErrorEnum::DeltaOverflow { position })
    }

    /// A [`CodecErrorEnum::InvalidId`] for the ID at `position`, with why it is not valid.
    #[must_use]
    pub fn invalid_id(position: usize, error: IdError) -> Self {
        Self(CodecErrorEnum::InvalidId { position, error })
    }

    /// A [`CodecErrorEnum::TrailingDeltas`] for the bytes starting at `offset`.
    #[must_use]
    pub fn trailing_deltas(offset: usize) -> Self {
        Self(CodecErrorEnum::TrailingDeltas { offset })
    }

    /// A [`CodecErrorEnum::Truncated`] for the field starting at `offset`.
    #[must_use]
    pub fn truncated(offset: usize) -> Self {
        Self(CodecErrorEnum::Truncated { offset })
    }

    /// A [`CodecErrorEnum::UnknownVersion`] for `version`.
    #[must_use]
    pub fn unknown_version(version: u8) -> Self {
        Self(CodecErrorEnum::UnknownVersion { version })
    }

    /// A [`CodecErrorEnum::TrailingBytes`] for the bytes starting at `offset`.
    #[must_use]
    pub fn trailing_bytes(offset: usize) -> Self {
        Self(CodecErrorEnum::TrailingBytes { offset })
    }

    /// A [`CodecErrorEnum::BlocksOutOfOrder`] for the block at `block`.
    #[must_use]
    pub fn blocks_out_of_order(block: usize) -> Self {
        Self(CodecErrorEnum::BlocksOutOfOrder { block })
    }
}
