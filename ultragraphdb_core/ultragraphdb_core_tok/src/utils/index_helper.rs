/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index/helper.go. Changed for ultragraphdb: translated to Rust; Go reinterprets
 * the bytes in place with unsafe, this port decodes them.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_traits_index::Float;

/// Dgraph's `BytesToFloat`: the little-endian float of `float_bits` (32 or 64) bits at the start of `encoded`, as
/// `T`.
///
/// # Panics
///
/// On any other width, as Go panics with `Invalid floatBits`, and when `encoded` is shorter than the width.
pub fn bytes_to_float<T: Float>(encoded: &[u8], float_bits: usize) -> T {
    match float_bits {
        32 => T::from_f32(f32::from_le_bytes(
            encoded[..4].try_into().expect("4 bytes"),
        )),
        64 => T::from_f64(f64::from_le_bytes(
            encoded[..8].try_into().expect("8 bytes"),
        )),
        _ => panic!("Invalid floatBits"),
    }
}

/// Dgraph's `BytesAsFloatArray`: `encoded` as floats of `float_bits` bits, written to `ret_val`. Empty input gives
/// an empty vector; input whose length is not a multiple of the float width leaves `ret_val` as it is (Go logs the
/// error).
pub fn bytes_as_float_array<T: Float>(encoded: &[u8], ret_val: &mut Vec<T>, float_bits: usize) {
    let float_bytes = float_bits / 8;
    if encoded.is_empty() {
        ret_val.clear();
        return;
    }
    if !encoded.len().is_multiple_of(float_bytes) {
        return;
    }
    ret_val.clear();
    ret_val.extend(
        encoded
            .chunks_exact(float_bytes)
            .map(|c| bytes_to_float::<T>(c, float_bits)),
    );
}
