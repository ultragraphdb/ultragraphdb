/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Unsigned LEB128: seven bits a byte, the least significant group first, the high bit set on every byte but the
//! last. Data plane: a write appends to the caller's buffer and a read borrows its bytes.

use crate::errors::codec_error::CodecError;

/// The most bytes a 128-bit value takes, `ceil(128 / 7)`.
pub(crate) const U128_LEN_MAX: usize = 19;

const _: () = assert!(U128_LEN_MAX * 7 >= 128 && (U128_LEN_MAX - 1) * 7 < 128);

/// Appends `value` to `w`, in its shortest form.
pub(crate) fn put(w: &mut Vec<u8>, value: u128) {
    let start = w.len();
    let mut rest = value;
    // At most U128_LEN_MAX groups: each pass shifts seven bits out of a 128-bit value.
    for _ in 0..U128_LEN_MAX {
        let [.., low] = rest.to_be_bytes();
        let group = low & 0x7F;
        rest >>= 7;
        if rest == 0 {
            w.push(group);
            break;
        }
        w.push(group | 0x80);
    }
    debug_assert_eq!(rest, 0, "a 128-bit value takes at most 19 groups");
    debug_assert!(w.len() > start, "a value takes at least one byte");
}

/// The value at the start of `bytes`, at most `len_max` bytes long, and the bytes it took. `offset` is where `bytes`
/// starts in what is being read, for the errors.
///
/// # Errors
///
/// [`CodecError::truncated_varint`] when the bytes end inside the value, [`CodecError::overflowing_varint`] when it
/// is longer than `len_max` bytes or past 128 bits, and [`CodecError::non_canonical_varint`] when its last group is a
/// redundant zero.
pub(crate) fn read(
    bytes: &[u8],
    offset: usize,
    len_max: usize,
) -> Result<(u128, usize), CodecError> {
    debug_assert!(len_max <= U128_LEN_MAX, "no width needs more than 19 bytes");
    let mut value = 0u128;
    // The n-th group, n from 1, shifts by 7 × (n - 1); 19 shifts cover 128 bits, the most any width takes.
    let groups = bytes.iter().zip((0..u128::BITS).step_by(7));
    for (len, (&byte, shift)) in (1usize..).zip(groups).take(len_max) {
        let group = u128::from(byte & 0x7F);
        // The group's bits pass bit 128 exactly when it has fewer leading zeros than its shift.
        if group.leading_zeros() < shift {
            return Err(CodecError::overflowing_varint(offset));
        }
        value |= group << shift;
        if byte & 0x80 == 0 {
            if byte == 0 && len > 1 {
                return Err(CodecError::non_canonical_varint(offset));
            }
            return Ok((value, len));
        }
    }
    if bytes.len() < len_max {
        Err(CodecError::truncated_varint(offset))
    } else {
        Err(CodecError::overflowing_varint(offset))
    }
}
