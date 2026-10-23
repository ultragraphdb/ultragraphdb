/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The width of a vector index's floats: Dgraph's `floatBits`, an int that is 32 or 64 and on any other value
/// panics with `Invalid floatBits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatBits {
    F32,
    F64,
}

impl FloatBits {
    /// The width in bits.
    pub fn bits(self) -> usize {
        match self {
            FloatBits::F32 => 32,
            FloatBits::F64 => 64,
        }
    }
}
