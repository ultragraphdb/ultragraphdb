/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.27's `strings.ToLower`.

use ultragraphdb_common_utils::go_strings;

#[test]
fn test_to_lower_maps_each_rune_simply() {
    let cases = [
        ("ABC", "abc"),
        ("İSTANBUL", "istanbul"),
        ("ΣΑΣ", "σασ"),
        ("Straße", "straße"),
        ("ǅ", "ǆ"),
        ("ÀÉÎ", "àéî"),
        ("ⅫⒶ", "ⅻⓐ"),
        ("", ""),
        // Case pairs Unicode 16 added, which Go 1.27 (Unicode 17.0.0) maps and Go 1.26 left unchanged.
        ("\u{a7cb}\u{a7dc}\u{1c89}", "\u{264}\u{19b}\u{1c8a}"),
        ("\u{10d50}\u{16ea0}", "\u{10d70}\u{16ebb}"),
    ];
    for (s, want) in cases {
        assert_eq!(go_strings::to_lower(s), want, "{s:?}");
    }
}
