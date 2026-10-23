/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::{FloatBits, TokErrorEnum, bytes_as_float_array, bytes_to_float};

#[test]
fn test_bytes_to_float() {
    assert_eq!(
        bytes_to_float::<f32>(&1.5f32.to_le_bytes(), FloatBits::F32),
        Ok(1.5)
    );
    assert_eq!(
        bytes_to_float::<f64>(&2.25f64.to_le_bytes(), FloatBits::F64),
        Ok(2.25)
    );
    assert_eq!(
        bytes_to_float::<f64>(&0.5f32.to_le_bytes(), FloatBits::F32),
        Ok(0.5)
    );
    // Bytes past the width are not read, as in Go.
    assert_eq!(
        bytes_to_float::<f32>(&[0, 0, 0xc0, 0x3f, 9], FloatBits::F32),
        Ok(1.5)
    );
}

#[test]
fn test_bytes_to_float_of_too_few_bytes_is_refused() {
    // Go's binary.LittleEndian panics indexing past the input; a width other than 32 and 64, on which Go panics
    // with "Invalid floatBits", is not a FloatBits.
    let err = bytes_to_float::<f64>(&[1, 2, 3], FloatBits::F64).expect_err("three bytes");
    assert_eq!(err.0, TokErrorEnum::ShortFloat { len: 3, bits: 64 });
    assert_eq!(err.to_string(), "cannot read a 64-bit float from 3 bytes");
    assert!(bytes_to_float::<f32>(&[], FloatBits::F32).is_err());
    assert_eq!((FloatBits::F32.bits(), FloatBits::F64.bits()), (32, 64));
}

#[test]
fn test_bytes_as_float_array() {
    let bytes: Vec<u8> = [1.0f32, -2.0, 3.5]
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let mut out = vec![9.0f32];
    bytes_as_float_array(&bytes, &mut out, FloatBits::F32);
    assert_eq!(out, vec![1.0, -2.0, 3.5]);
    bytes_as_float_array(&bytes[..5], &mut out, FloatBits::F32);
    assert_eq!(
        out,
        vec![1.0, -2.0, 3.5],
        "a partial float leaves the result as it was"
    );
    let mut wide: Vec<f64> = Vec::new();
    bytes_as_float_array(&bytes[..8], &mut wide, FloatBits::F64);
    assert_eq!(wide.len(), 1);
    bytes_as_float_array(&[], &mut out, FloatBits::F32);
    assert!(out.is_empty());
}
