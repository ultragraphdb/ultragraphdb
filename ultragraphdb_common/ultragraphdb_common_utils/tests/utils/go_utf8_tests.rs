/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.26's `utf8.DecodeRune` and `bytes.Runes`.

use std::borrow::Cow;

use ultragraphdb_common_utils::go_utf8;

#[test]
fn test_decode_rune_and_runes() {
    // bytes, DecodeRune's rune and width, string(bytes.Runes(bytes))
    let cases: [(&[u8], u32, usize, &str); 10] = [
        (b"", 0xfffd, 0, ""),
        (b"a", 0x61, 1, "a"),
        (b"\xc3\xa9", 0xe9, 2, "é"),
        (b"\xe2\x82\xac", 0x20ac, 3, "€"),
        (b"\xe2\x82", 0xfffd, 1, "\u{fffd}\u{fffd}"),
        (b"\xc0\x80", 0xfffd, 1, "\u{fffd}\u{fffd}"),
        (b"\xed\xa0\x80", 0xfffd, 1, "\u{fffd}\u{fffd}\u{fffd}"),
        (
            b"\xf4\x90\x80\x80",
            0xfffd,
            1,
            "\u{fffd}\u{fffd}\u{fffd}\u{fffd}",
        ),
        (b"\xf0\x9f\x98\x80", 0x1f600, 4, "\u{1f600}"),
        (b"\xff", 0xfffd, 1, "\u{fffd}"),
    ];
    for (b, rune, width, text) in cases {
        assert_eq!(go_utf8::decode_rune(b), (rune, width), "{b:?}");
        assert_eq!(
            go_utf8::runes(b).into_iter().collect::<String>(),
            text,
            "{b:?}"
        );
        assert_eq!(go_utf8::go_string(b), text, "{b:?}");
    }
    assert_eq!(go_utf8::RUNE_ERROR, 0xfffd);
}

#[test]
fn test_go_string_borrows_valid_utf8() {
    assert!(matches!(go_utf8::go_string(b"abc"), Cow::Borrowed("abc")));
    assert!(matches!(go_utf8::go_string(b"a\xffb"), Cow::Owned(s) if s == "a\u{fffd}b"));
}

#[test]
fn test_encode_is_string_of_runes() {
    assert_eq!(
        go_utf8::encode(&['a', 'é', '\u{1f600}']),
        "aé\u{1f600}".as_bytes()
    );
    assert_eq!(go_utf8::encode(&[]), b"");
}
