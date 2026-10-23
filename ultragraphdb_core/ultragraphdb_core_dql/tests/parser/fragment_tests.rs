/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The blocks fragments expand into count toward the nesting bound of 1,000 levels (ledger 32).

use crate::test_helpers::check_depth_bound;

/// `{ q(func: uid(1)) { ...f1 } } fragment f1 { a { ...f2 } } ... fragment fn { a }`: each fragment one level
/// deeper.
fn fragment_chain(depth: usize) -> String {
    let n = depth - 1;
    let mut q = String::from("{ q(func: uid(1)) { ...f1 } }");
    for i in 1..n {
        q.push_str(&format!(" fragment f{i} {{ a {{ ...f{} }} }}", i + 1));
    }
    q.push_str(&format!(" fragment f{n} {{ a }}"));
    q
}

#[test]
fn test_expanded_fragments_nest_at_most_1000_levels() {
    check_depth_bound(fragment_chain);
}

#[test]
fn test_a_fragment_spread_deeper_than_its_first_use_is_refused() {
    // f holds 500 levels; spread at level 2 it fits, spread again at level 502 it reaches 1,001.
    let f = format!(
        "fragment f {{ {}a{} }}",
        "a { ".repeat(499),
        " }".repeat(499)
    );
    let ok = format!("{{ q(func: uid(1)) {{ ...f }} }} {f}");
    assert_eq!(crate::test_helpers::parse_deep(ok), Ok(501));
    let deep = format!(
        "{{ q(func: uid(1)) {{ ...f {}...f{} }} }} {f}",
        "b { ".repeat(500),
        " }".repeat(500)
    );
    assert_eq!(
        crate::test_helpers::parse_deep(deep),
        Err(crate::test_helpers::TOO_DEEP.to_string())
    );
}
