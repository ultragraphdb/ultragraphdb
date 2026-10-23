/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::{Display, Formatter, Result};

use super::{CodecError, CodecErrorEnum};

impl Display for CodecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.0 {
            CodecErrorEnum::TruncatedVarint { offset } => {
                write!(f, "truncated uvarint at byte {offset} of the buffer")
            }
            CodecErrorEnum::OverflowingVarint { offset } => {
                write!(f, "uvarint at byte {offset} of the buffer overflows a u64")
            }
        }
    }
}
