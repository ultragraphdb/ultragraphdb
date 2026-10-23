/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{is_end_of_line, is_esc_char};

#[test]
fn test_is_end_of_line() {
    assert!(is_end_of_line('\n'));
    assert!(is_end_of_line('\r'));
    for r in [' ', '\t', 'a', '\u{2028}', '\u{85}', '\u{b}', '\u{c}'] {
        assert!(!is_end_of_line(r), "rune {r:?}");
    }
}

#[test]
fn test_is_esc_char() {
    for r in ['u', 'v', 't', 'b', 'n', 'r', 'f', '"', '\'', '\\'] {
        assert!(is_esc_char(Some(r)), "rune {r:?}");
    }
    for r in ['U', 'x', 'a', '0', '/', ' '] {
        assert!(!is_esc_char(Some(r)), "rune {r:?}");
    }
    assert!(!is_esc_char(None));
}
