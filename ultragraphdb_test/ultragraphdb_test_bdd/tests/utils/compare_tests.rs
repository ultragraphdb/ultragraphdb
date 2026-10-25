/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_test_bdd::compare_bytes;

#[test]
fn equal_bytes_compare_equal() {
    let cases: &[(&str, &[u8])] = &[
        ("x", &[1, 2]),
        ("", &[]),
        ("all", &[0x00, 0x7f, 0x80, 0xff]),
    ];
    for (input, bytes) in cases {
        assert_eq!(
            compare_bytes(input, bytes, bytes),
            Ok(()),
            "input {input:?}"
        );
    }
}

#[test]
fn a_mismatch_reports_the_input_and_both_values_in_hex() {
    // Each expected message is written out by hand from the documented format: the input as a Rust string literal,
    // then each value as two lowercase hex digits per byte, separated by single spaces.
    let cases: &[(&str, &[u8], &[u8], &str)] = &[
        (
            "in",
            &[0x0a],
            &[0xff],
            "golden mismatch for input \"in\"\n  expected: 0a\n  actual:   ff",
        ),
        // A prefix is not a match: the longer actual differs.
        (
            "p",
            &[0x00, 0x01],
            &[0x00, 0x01, 0x02],
            "golden mismatch for input \"p\"\n  expected: 00 01\n  actual:   00 01 02",
        ),
        // A truncated actual differs as well.
        (
            "t",
            &[0x00, 0x01, 0x02],
            &[0x00, 0x01],
            "golden mismatch for input \"t\"\n  expected: 00 01 02\n  actual:   00 01",
        ),
        // An empty value prints as nothing after its label.
        (
            "",
            &[],
            &[0x00],
            "golden mismatch for input \"\"\n  expected: \n  actual:   00",
        ),
        (
            "e",
            &[0x10],
            &[],
            "golden mismatch for input \"e\"\n  expected: 10\n  actual:   ",
        ),
        // Every byte is zero-padded to two lowercase digits, across the whole range.
        (
            "range",
            &[0x00, 0x09, 0x0f, 0x7f],
            &[0x80, 0xa0, 0xfe, 0xff],
            "golden mismatch for input \"range\"\n  expected: 00 09 0f 7f\n  actual:   80 a0 fe ff",
        ),
        // The input is quoted and escaped, so a newline or quote in it cannot be mistaken for the message's own.
        (
            "a\"b\nä",
            &[0x01],
            &[0x02],
            "golden mismatch for input \"a\\\"b\\nä\"\n  expected: 01\n  actual:   02",
        ),
    ];
    for (input, expected, actual, message) in cases {
        assert_eq!(
            compare_bytes(input, expected, actual),
            Err((*message).to_string()),
            "input {input:?}"
        );
    }
}

#[test]
fn a_mismatch_in_a_large_value_prints_every_byte() {
    let expected: Vec<u8> = (0..=255).collect();
    let mut actual = expected.clone();
    actual[255] = 0x00;
    let err = compare_bytes("large", &expected, &actual).unwrap_err();
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 3, "{err}");
    let expected_hex = lines[1].strip_prefix("  expected: ").unwrap();
    let actual_hex = lines[2].strip_prefix("  actual:   ").unwrap();
    // 256 bytes are 256 two-digit groups and 255 separators.
    assert_eq!(expected_hex.len(), 256 * 3 - 1);
    assert_eq!(actual_hex.len(), 256 * 3 - 1);
    assert!(expected_hex.starts_with("00 01 02 "), "{expected_hex}");
    assert!(expected_hex.ends_with(" fd fe ff"), "{expected_hex}");
    assert!(actual_hex.ends_with(" fd fe 00"), "{actual_hex}");
}
