/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use ultragraphdb_core_text::{Collator, Tag};

fn collator(tag: &str) -> Collator {
    Collator::new(&Tag::parse(tag).unwrap())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn an_invalid_byte_weighs_as_the_implicit_weight_of_u_fffd() {
    let und = collator("und");
    assert_eq!(hex(&und.key_bytes(b"\x80")), "85fffd00000020000002");
    assert_eq!(hex(&und.key_bytes(b"\xff")), "85fffd00000020000002");
    // U+FFFD itself has a weight of its own in the table.
    assert_eq!(
        hex(&und.key_bytes("\u{fffd}".as_bytes())),
        "13d800000020000002"
    );
}

#[test]
fn a_complete_invalid_sequence_weighs_as_one_u_fffd() {
    let und = collator("und");
    let one = und.key_bytes(b"\x80");
    // Overlong encodings, surrogates and code points past U+10FFFF: the trie reads every byte of the sequence.
    for s in [
        &b"\xc0\x80"[..],
        b"\xe0\x80\x80",
        b"\xed\xa0\x80",
        b"\xf0\x80\x80\x80",
        b"\xf4\x90\x80\x80",
        b"\xf7\xbf\xbf\xbf",
    ] {
        assert_eq!(und.key_bytes(s), one, "{s:x?}");
    }
}

#[test]
fn a_broken_sequence_weighs_as_one_u_fffd_up_to_the_breaking_byte() {
    let und = collator("und");
    // Two bytes of a three-byte sequence, then a byte that does not continue it: one U+FFFD, then 'x'.
    assert_eq!(und.key_bytes(b"\xe0\xa0x"), und.key_bytes(b"\x80x"));
    // The same two bytes at the end of the input are too few to look up: each byte weighs as U+FFFD.
    assert_eq!(und.key_bytes(b"\xe0\xa0"), und.key_bytes(b"\x80\x80"));
}

#[test]
fn text_keys_and_compares_as_its_bytes() {
    let de = collator("de");
    for s in ["", "Äpfel", "Straße", "a\u{301}", "中文"] {
        assert_eq!(de.key(s), de.key_bytes(s.as_bytes()), "{s:?}");
        assert_eq!(
            de.compare(s, "Birne"),
            de.compare_bytes(s.as_bytes(), b"Birne"),
            "{s:?}"
        );
    }
}

#[test]
fn invalid_bytes_differ_only_at_the_identical_level() {
    assert_eq!(
        collator("en").compare_bytes(b"\xfe", b"\xff"),
        Ordering::Equal
    );
    assert_eq!(
        collator("en-u-ks-identic").compare_bytes(b"\xfe", b"\xff"),
        Ordering::Less
    );
    assert_eq!(
        collator("en").compare_bytes(b"a\xffb", b"ab"),
        Ordering::Greater
    );
}
