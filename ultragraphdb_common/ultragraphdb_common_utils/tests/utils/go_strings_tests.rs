/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.26's `strings.ToLower`.

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
    ];
    for (s, want) in cases {
        assert_eq!(go_strings::to_lower(s), want, "{s:?}");
    }
}
