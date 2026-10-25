/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.27's `unicode` package, at Unicode 17.0.0.

use ultragraphdb_common_utils::go_unicode;

#[test]
fn test_letter_digit_ideographic_and_lower() {
    // rune, IsLetter, IsDigit, Is(Ideographic), ToLower
    let cases: [(u32, bool, bool, bool, u32); 12] = [
        (0x61, true, false, false, 0x61),
        (0x31, false, true, false, 0x31),
        (0x5f, false, false, false, 0x5f),
        (0x661, false, true, false, 0x661),
        (0x4e00, true, false, true, 0x4e00),
        (0x3007, false, false, true, 0x3007),
        (0xff10, false, true, false, 0xff10),
        (0x1f600, false, false, false, 0x1f600),
        (0x10ffff, false, false, false, 0x10ffff),
        (0x130, true, false, false, 0x69),
        (0x3a3, true, false, false, 0x3c3),
        (0x2160, false, false, false, 0x2170),
    ];
    for (r, letter, digit, ideographic, lower) in cases {
        assert_eq!(go_unicode::is_letter(r), letter, "{r:#x}");
        assert_eq!(go_unicode::is_digit(r), digit, "{r:#x}");
        assert_eq!(go_unicode::is_ideographic(r), ideographic, "{r:#x}");
        assert_eq!(go_unicode::to_lower(r), lower, "{r:#x}");
    }
}

#[test]
fn test_takes_chars_as_runes() {
    assert!(go_unicode::is_letter('é'));
    assert!(go_unicode::is_digit('٣'));
    assert!(go_unicode::is_ideographic('中'));
    assert_eq!(go_unicode::to_lower('Q'), u32::from('q'));
}

#[test]
fn test_beyond_unicode_is_nothing() {
    assert!(!go_unicode::is_letter(0x110000u32));
    assert!(!go_unicode::is_digit(u32::MAX));
    assert!(!go_unicode::is_ideographic(0x110000u32));
    assert_eq!(go_unicode::to_lower(0x110000u32), 0x110000);
}

#[test]
fn test_letter_digit_ideographic_and_lower_are_unicode_17() {
    // Go 1.27 (Unicode 17.0.0), which builds Dgraph, on runes Unicode 16 and 17 assigned: letters, digits and
    // ideographs, with the lower case Unicode 17 maps each to.
    // rune, IsLetter, IsDigit, Is(Ideographic), ToLower
    let cases: [(u32, bool, bool, bool, u32); 14] = [
        (0xa7cb, true, false, false, 0x264),
        (0xa7dc, true, false, false, 0x19b),
        (0x1c89, true, false, false, 0x1c8a),
        (0x10d50, true, false, false, 0x10d70),
        (0x16ea0, true, false, false, 0x16ebb),
        (0x16d40, true, false, false, 0x16d40),
        (0x10ec5, true, false, false, 0x10ec5),
        (0x10d40, false, true, false, 0x10d40),
        (0x16d70, false, true, false, 0x16d70),
        (0x1ccf0, false, true, false, 0x1ccf0),
        (0x116d0, false, true, false, 0x116d0),
        (0x2eda3, true, false, true, 0x2eda3),
        (0x323b0, true, false, true, 0x323b0),
        (0x18d1e, true, false, true, 0x18d1e),
    ];
    for (r, letter, digit, ideographic, lower) in cases {
        assert_eq!(go_unicode::is_letter(r), letter, "{r:#x}");
        assert_eq!(go_unicode::is_digit(r), digit, "{r:#x}");
        assert_eq!(go_unicode::is_ideographic(r), ideographic, "{r:#x}");
        assert_eq!(go_unicode::to_lower(r), lower, "{r:#x}");
    }
}
