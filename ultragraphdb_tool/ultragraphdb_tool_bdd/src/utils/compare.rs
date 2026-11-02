/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::test_error::{TestError, TestResult};

/// Compares an output with its golden vector.
///
/// # Errors
///
/// A [`TestError`] on a mismatch, naming the input and both values in hex.
pub fn compare_bytes(input: &str, expected: &[u8], actual: &[u8]) -> TestResult {
    if expected == actual {
        return Ok(());
    }
    Err(TestError::new(format!(
        "golden mismatch for input {input:?}\n  expected: {}\n  actual:   {}",
        hex(expected),
        hex(actual)
    )))
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
