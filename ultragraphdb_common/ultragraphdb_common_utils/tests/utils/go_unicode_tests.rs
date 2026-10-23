/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go's `unicode` package, on runes whose properties Unicode 15 and 17 agree on.

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
