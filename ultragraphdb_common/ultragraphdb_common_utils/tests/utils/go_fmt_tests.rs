/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.27's `fmt.Sprintf`.

use ultragraphdb_common_utils::go_fmt;

#[test]
fn test_byte_slice_is_percent_v() {
    assert_eq!(go_fmt::byte_slice(&[]), "[]");
    assert_eq!(go_fmt::byte_slice(&[1, 2, 255]), "[1 2 255]");
}

#[test]
fn test_string_slice_is_percent_v() {
    assert_eq!(go_fmt::string_slice(&[]), "[]");
    assert_eq!(
        go_fmt::string_slice(&["a".to_string(), "b c".to_string()]),
        "[a b c]"
    );
}

#[test]
fn test_unicode_hash_is_percent_sharp_u() {
    let cases: [(char, &str); 11] = [
        ('A', "U+0041 'A'"),
        ('\u{0}', "U+0000"),
        (' ', "U+0020 ' '"),
        ('\u{7f}', "U+007F"),
        ('\u{a0}', "U+00A0"),
        ('\u{ad}', "U+00AD"),
        ('\u{2028}', "U+2028"),
        ('\u{fffd}', "U+FFFD '\u{fffd}'"),
        ('\u{1f600}', "U+1F600 '\u{1f600}'"),
        // Assigned in Unicode 17, which Go 1.27's IsPrint knows.
        ('\u{33392}', "U+33392 '\u{33392}'"),
        ('\u{10ffff}', "U+10FFFF"),
    ];
    for (r, want) in cases {
        assert_eq!(go_fmt::unicode_hash(Some(r)), want, "{r:?}");
    }
    // The end of the input is Go's rune -1.
    assert_eq!(go_fmt::unicode_hash(None), "U+FFFFFFFFFFFFFFFF");
}
