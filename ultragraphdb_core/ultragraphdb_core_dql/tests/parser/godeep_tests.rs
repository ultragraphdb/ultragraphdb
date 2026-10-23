/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Selection blocks nest at most 1,000 levels deep (ledger 32).

use crate::test_helpers::check_depth_bound;

/// `{ q(func: uid(1)) { a { a { ... a } } } }`, `depth` levels of blocks.
fn nested_blocks(depth: usize) -> String {
    let inner = depth - 2;
    format!(
        "{{ q(func: uid(1)) {{ {}a{} }} }}",
        "a { ".repeat(inner),
        " }".repeat(inner)
    )
}

#[test]
fn test_selection_blocks_nest_at_most_1000_levels() {
    check_depth_bound(nested_blocks);
}

#[test]
fn test_a_filter_counts_below_its_block() {
    // The filter of the innermost block sits one level below it.
    check_depth_bound(|depth| {
        let inner = depth - 3;
        format!(
            "{{ q(func: uid(1)) {{ {}a @filter(eq(a, 1)){} }} }}",
            "a { ".repeat(inner),
            " }".repeat(inner)
        )
    });
}
