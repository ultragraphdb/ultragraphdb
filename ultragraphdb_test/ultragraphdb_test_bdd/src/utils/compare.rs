/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Compares an output with its golden vector; on a mismatch, the message names the input and both values in hex.
pub fn compare_bytes(input: &str, expected: &[u8], actual: &[u8]) -> Result<(), String> {
    if expected == actual {
        return Ok(());
    }
    Err(format!(
        "golden mismatch for input {input:?}\n  expected: {}\n  actual:   {}",
        hex(expected),
        hex(actual)
    ))
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
