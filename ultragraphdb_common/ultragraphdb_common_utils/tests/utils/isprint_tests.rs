/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.26's `strconv.IsPrint`.

use ultragraphdb_common_utils::go_strconv::is_print;

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
