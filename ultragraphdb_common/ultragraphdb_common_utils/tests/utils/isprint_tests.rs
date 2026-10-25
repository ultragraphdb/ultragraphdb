/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.27's `strconv.IsPrint` and `strconv.QuoteRune`, at Unicode 17.0.0: Go 1.27 builds
//! Dgraph (its go.mod says `go 1.27.0`), and its tables accept 159613 runes.

use ultragraphdb_common_utils::go_strconv::{self, is_print};

#[test]
fn test_is_print_follows_gos_tables() {
    let cases: [(char, bool); 15] = [
        ('\u{20}', true),
        ('\u{61}', true),
        ('\u{7e}', true),
        ('\u{7f}', false),
        ('\u{a0}', false),
        ('\u{a1}', true),
        ('\u{ad}', false),
        ('\u{378}', false),
        ('\u{2028}', false),
        ('\u{1f600}', true),
        ('\u{e000}', false),
        ('\u{e0001}', false),
        ('\u{20000}', true),
        ('\u{3134a}', true),
        ('\u{fffd}', true),
    ];
    for (r, want) in cases {
        assert_eq!(is_print(r), want, "{r:?}");
    }
}

#[test]
fn test_is_print_is_unicode_17() {
    // rune, IsPrint, QuoteRune: runes Unicode 16 and 17 assigned, which Go 1.27 prints, and the ends of the planes
    // beside them.
    let cases: [(char, bool, &str); 21] = [
        ('\u{1ae1}', true, "'\u{1ae1}'"),
        ('\u{1c89}', true, "'\u{1c89}'"),
        ('\u{a7cb}', true, "'\u{a7cb}'"),
        ('\u{fbc7}', true, "'\u{fbc7}'"),
        ('\u{10d40}', true, "'\u{10d40}'"),
        ('\u{138c2}', true, "'\u{138c2}'"),
        ('\u{141ff}', true, "'\u{141ff}'"),
        ('\u{16d40}', true, "'\u{16d40}'"),
        ('\u{1f8c0}', true, "'\u{1f8c0}'"),
        ('\u{2eda3}', true, "'\u{2eda3}'"),
        ('\u{31350}', true, "'\u{31350}'"),
        ('\u{32461}', true, "'\u{32461}'"),
        ('\u{33392}', true, "'\u{33392}'"),
        ('\u{323b0}', true, "'\u{323b0}'"),
        ('\u{3134a}', true, "'\u{3134a}'"),
        ('\u{e0100}', true, "'\u{e0100}'"),
        ('\u{3347a}', false, r"'\U0003347a'"),
        ('\u{3134b}', false, r"'\U0003134b'"),
        ('\u{10ffff}', false, r"'\U0010ffff'"),
        ('\u{2fffd}', false, r"'\U0002fffd'"),
        ('\u{1aff}', false, "'\\u1aff'"),
    ];
    for (r, print, quoted) in cases {
        assert_eq!(is_print(r), print, "{r:?}");
        assert_eq!(go_strconv::quote_rune(r), quoted, "{r:?}");
    }
}

#[test]
fn test_is_print_counts_gos_printable_runes() {
    // Go 1.27: the number of runes in 0..=0x10FFFF that strconv.IsPrint accepts.
    let printable = (0..=0x10ffffu32)
        .filter_map(char::from_u32)
        .filter(|&c| is_print(c))
        .count();
    assert_eq!(printable, 159613);
}
