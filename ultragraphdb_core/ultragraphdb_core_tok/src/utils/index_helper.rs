/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index/helper.go. Changed for ultragraphdb: translated to Rust; Go reinterprets
 * the bytes in place with unsafe, this port decodes them.
 * Copyright (c) 2026. Marvin Hansen.
 */

use tracing::error;
use ultragraphdb_traits_index::Float;

use crate::errors::tok_error::TokError;
use crate::types::float_bits::FloatBits;

/// Dgraph's `BytesToFloat`: the little-endian float of `float_bits` at the start of `encoded`, as `T`. A width
/// other than 32 and 64, on which Go panics with `Invalid floatBits`, is no [`FloatBits`].
///
/// # Errors
///
/// [`TokErrorEnum::ShortFloat`](crate::TokErrorEnum::ShortFloat) when `encoded` is shorter than the width, on
/// which Go panics.
pub fn bytes_to_float<T: Float>(encoded: &[u8], float_bits: FloatBits) -> Result<T, TokError> {
    let short = || TokError::ShortFloat(encoded.len(), float_bits.bits());
    match float_bits {
        FloatBits::F32 => encoded
            .first_chunk::<4>()
            .map(|b| T::from_f32(f32::from_le_bytes(*b)))
            .ok_or_else(short),
        FloatBits::F64 => encoded
            .first_chunk::<8>()
            .map(|b| T::from_f64(f64::from_le_bytes(*b)))
            .ok_or_else(short),
    }
}

/// Dgraph's `BytesAsFloatArray`: `encoded` as floats of `float_bits`, written to `ret_val`. Empty input gives an
/// empty vector; input whose length is not a multiple of the float width leaves `ret_val` as it is and is logged as
/// an error, as Go logs it.
pub fn bytes_as_float_array<T: Float>(encoded: &[u8], ret_val: &mut Vec<T>, float_bits: FloatBits) {
    if encoded.is_empty() {
        ret_val.clear();
        return;
    }
    match float_bits {
        FloatBits::F32 => match encoded.as_chunks::<4>() {
            (floats, []) => {
                ret_val.clear();
                ret_val.extend(floats.iter().map(|b| T::from_f32(f32::from_le_bytes(*b))));
            }
            _ => log_invalid_length(encoded),
        },
        FloatBits::F64 => match encoded.as_chunks::<8>() {
            (floats, []) => {
                ret_val.clear();
                ret_val.extend(floats.iter().map(|b| T::from_f64(f64::from_le_bytes(*b))));
            }
            _ => log_invalid_length(encoded),
        },
    }
}

fn log_invalid_length(encoded: &[u8]) {
    error!(len = encoded.len(), encoded = ?encoded, "Invalid byte slice length");
}
