/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.26's `fmt.Sprintf`.

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
    assert_eq!(go_fmt::unicode_hash(Some('A')), "U+0041 'A'");
    assert_eq!(go_fmt::unicode_hash(Some('\u{7f}')), "U+007F");
    assert_eq!(
        go_fmt::unicode_hash(Some('\u{1f600}')),
        "U+1F600 '\u{1f600}'"
    );
    // The end of the input is Go's rune -1.
    assert_eq!(go_fmt::unicode_hash(None), "U+FFFFFFFFFFFFFFFF");
}
