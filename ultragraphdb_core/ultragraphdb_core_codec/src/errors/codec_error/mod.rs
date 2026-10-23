/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod codec_error_display;
mod codec_error_error;

/// Why a buffer of uvarint deltas could not be packed. Go's `EncodeFromBuffer` has no error: it never returns on a
/// truncated varint and panics on an overlong one.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodecError(pub CodecErrorEnum);

/// The classification of a [`CodecError`]. Each `offset` is the byte at which the varint starts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CodecErrorEnum {
    /// The buffer ends inside a varint.
    TruncatedVarint { offset: usize },
    /// The varint is longer than ten bytes or its value exceeds `u64::MAX`.
    OverflowingVarint { offset: usize },
}

impl CodecError {
    #[allow(non_snake_case)]
    pub fn TruncatedVarint(offset: usize) -> Self {
        Self(CodecErrorEnum::TruncatedVarint { offset })
    }

    #[allow(non_snake_case)]
    pub fn OverflowingVarint(offset: usize) -> Self {
        Self(CodecErrorEnum::OverflowingVarint { offset })
    }
}
