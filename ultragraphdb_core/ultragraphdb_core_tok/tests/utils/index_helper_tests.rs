/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::{bytes_as_float_array, bytes_to_float};

#[test]
fn test_bytes_to_float() {
    assert_eq!(bytes_to_float::<f32>(&1.5f32.to_le_bytes(), 32), 1.5);
    assert_eq!(bytes_to_float::<f64>(&2.25f64.to_le_bytes(), 64), 2.25);
    assert_eq!(bytes_to_float::<f64>(&0.5f32.to_le_bytes(), 32), 0.5);
}

#[test]
#[should_panic(expected = "Invalid floatBits")]
fn test_bytes_to_float_invalid_bits() {
    let _ = bytes_to_float::<f32>(&[0; 8], 16);
}

#[test]
fn test_bytes_as_float_array() {
    let bytes: Vec<u8> = [1.0f32, -2.0, 3.5]
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let mut out = vec![9.0f32];
    bytes_as_float_array(&bytes, &mut out, 32);
    assert_eq!(out, vec![1.0, -2.0, 3.5]);
    bytes_as_float_array(&bytes[..5], &mut out, 32);
    assert_eq!(
        out,
        vec![1.0, -2.0, 3.5],
        "a partial float leaves the result as it was"
    );
    bytes_as_float_array(&[], &mut out, 32);
    assert!(out.is_empty());
}
