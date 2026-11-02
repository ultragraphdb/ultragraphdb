/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::{Display, Formatter, Result};

use super::{CodecError, CodecErrorEnum};

impl Display for CodecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.0 {
            CodecErrorEnum::OutOfOrder { position } => write!(
                f,
                "the ID at position {position} is not greater than the one before it"
            ),
            CodecErrorEnum::TruncatedVarint { offset } => {
                write!(f, "truncated varint at byte {offset}")
            }
            CodecErrorEnum::OverflowingVarint { offset } => {
                write!(f, "varint at byte {offset} overflows its width")
            }
            CodecErrorEnum::NonCanonicalVarint { offset } => {
                write!(f, "varint at byte {offset} is not in its shortest form")
            }
            CodecErrorEnum::DeltaOverflow { position } => write!(
                f,
                "the delta of the ID at position {position} carries it past 104 bits"
            ),
            CodecErrorEnum::InvalidId { position, error } => {
                write!(f, "the ID at position {position} is invalid: {error}")
            }
            CodecErrorEnum::TrailingDeltas { offset } => {
                write!(f, "bytes after a block's last delta, at byte {offset}")
            }
            CodecErrorEnum::Truncated { offset } => {
                write!(f, "a pack's field at byte {offset} overruns its bytes")
            }
            CodecErrorEnum::UnknownVersion { version } => {
                write!(f, "unknown packed ID list version {version}")
            }
            CodecErrorEnum::TrailingBytes { offset } => {
                write!(f, "bytes after a pack's last block, at byte {offset}")
            }
            CodecErrorEnum::BlocksOutOfOrder { block } => {
                write!(f, "block {block} does not start above the block before it")
            }
        }
    }
}
