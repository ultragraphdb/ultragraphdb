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
    assert_eq!(
        bytes_to_float::<f64>(&f64::NEG_INFINITY.to_le_bytes(), FloatBits::F64),
        Ok(f64::NEG_INFINITY)
    );
    assert!(bytes_to_float::<f32>(&f32::NAN.to_le_bytes(), FloatBits::F32).is_ok_and(f32::is_nan));
    assert_eq!(
        bytes_to_float::<f32>(&[0, 0, 0, 0x80], FloatBits::F32).map(f32::to_bits),
        Ok(0x8000_0000),
        "negative zero keeps its sign"
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
    for (len, bits) in [
        (0, FloatBits::F32),
        (3, FloatBits::F32),
        (0, FloatBits::F64),
        (7, FloatBits::F64),
    ] {
        let err = bytes_to_float::<f64>(&vec![0; len], bits).expect_err("too few bytes");
        assert_eq!(
            err.0,
            TokErrorEnum::ShortFloat {
                len,
                bits: bits.bits()
            }
        );
    }
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
    // The eight bytes of 1.0f32 and -2.0f32, 00 00 80 3F 00 00 00 C0, read as one little-endian f64.
    let mut wide: Vec<f64> = vec![7.0, 7.0];
    bytes_as_float_array(&bytes[..8], &mut wide, FloatBits::F64);
    assert_eq!(wide, vec![f64::from_bits(0xC000_0000_3F80_0000)]);
    // A length that is a multiple of four but not of eight leaves an f64 result as it was.
    bytes_as_float_array(&bytes, &mut wide, FloatBits::F64);
    assert_eq!(wide, vec![f64::from_bits(0xC000_0000_3F80_0000)]);
    // f32 widths read into f64 are exact.
    let mut widened: Vec<f64> = Vec::new();
    bytes_as_float_array(&bytes, &mut widened, FloatBits::F32);
    assert_eq!(widened, vec![1.0, -2.0, 3.5]);
    bytes_as_float_array(&[], &mut out, FloatBits::F32);
    assert!(out.is_empty());
}

/// Dgraph's `BytesAsFloatArray` logs input whose length is not a multiple of the width, with its length and bytes.
#[test]
fn test_bytes_as_float_array_logs_a_length_not_a_multiple_of_the_width() {
    let capture = LogCapture::new();
    let mut floats = vec![9.0f32];
    capture.capture(|| {
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

/// The 64-bit width logs as the 32-bit one does: twelve bytes are three f32s but one and a half f64s.
#[test]
fn test_bytes_as_float_array_logs_a_length_not_a_multiple_of_eight() {
    let capture = LogCapture::new();
    let mut floats = vec![9.0f64];
    let twelve: Vec<u8> = (1..=12).collect();
    capture.capture(|| {
        bytes_as_float_array(&twelve, &mut floats, FloatBits::F64);
        bytes_as_float_array(&2.25f64.to_le_bytes(), &mut floats, FloatBits::F64);
    });
    assert_eq!(floats, vec![2.25]);
    let events = capture.events();
    assert_eq!(events.len(), 1, "only the refused input logs: {events:?}");
    assert_eq!(events[0].level(), Level::ERROR);
    assert_eq!(events[0].message(), Some("Invalid byte slice length"));
    assert_eq!(events[0].field("len"), Some("12"));
    assert_eq!(
        events[0].field("encoded"),
        Some("[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]")
    );
}
