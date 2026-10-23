/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use tracing::Level;
use ultragraphdb_core_tok::{FloatBits, TokErrorEnum, bytes_as_float_array, bytes_to_float};
use ultragraphdb_store_sim::LogCapture;

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

/// Dgraph's `BytesAsFloatArray` logs input whose length is not a multiple of the width, with its length and bytes.
#[test]
fn test_bytes_as_float_array_logs_a_length_not_a_multiple_of_the_width() {
    let capture = LogCapture::new();
    let mut floats = vec![9.0f32];
    tracing::subscriber::with_default(capture.subscriber(), || {
        bytes_as_float_array(&[1, 2, 3, 4, 5], &mut floats, FloatBits::F32);
        bytes_as_float_array(&1.5f32.to_le_bytes(), &mut floats, FloatBits::F32);
    });
    assert_eq!(floats, vec![1.5]);
    let events = capture.events();
    assert_eq!(events.len(), 1, "only the refused input logs: {events:?}");
    assert_eq!(events[0].level(), Level::ERROR);
    assert_eq!(events[0].message(), Some("Invalid byte slice length"));
    assert_eq!(events[0].field("len"), Some("5"));
    assert_eq!(events[0].field("encoded"), Some("[1, 2, 3, 4, 5]"));
}
