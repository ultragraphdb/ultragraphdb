/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's conversions of a position into the narrow fields a tag keeps it in: the low bits, as Go's `byte` and
//! `uint16` conversions take them, so a tag's offsets truncate exactly where Go's do.

/// Go's `byte(n)`: the low 8 bits of `n`.
pub(crate) fn go_byte(n: usize) -> u8 {
    let [low, ..] = n.to_le_bytes();
    low
}

/// Go's `uint16(n)`: the low 16 bits of `n`.
pub(crate) fn go_uint16(n: usize) -> u16 {
    let [low, high, ..] = n.to_le_bytes();
    u16::from_le_bytes([low, high])
}
