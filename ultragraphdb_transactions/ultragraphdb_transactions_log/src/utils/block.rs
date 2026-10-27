/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The block the log writes in: a round starts at a multiple of it and is padded to whole blocks, and an aligned file
/// takes offsets and lengths in multiples of it.
pub(crate) const BLOCK: usize = 4_096;

/// `len` rounded up to whole blocks, saturating.
pub(crate) fn padded(len: u64) -> u64 {
    len.div_ceil(BLOCK as u64).saturating_mul(BLOCK as u64)
}

/// Whether `offset` and `len` are both multiples of a block.
pub(crate) fn aligned(offset: u64, len: u64) -> bool {
    offset.is_multiple_of(BLOCK as u64) && len.is_multiple_of(BLOCK as u64)
}
