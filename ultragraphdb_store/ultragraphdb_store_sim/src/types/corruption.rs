/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How a stored value's bytes are corrupted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Corruption {
    /// Cut to one byte, too short for the version prefix: the store refuses the read with `Corrupted`.
    Truncate,
    /// One byte of the value, past the version prefix, inverted: the store returns the bytes, and the decoder
    /// above it must refuse them. A value too short to hold a byte past the prefix is truncated instead.
    FlipValueByte,
}
