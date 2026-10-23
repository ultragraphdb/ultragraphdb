/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The brackets of a filter count toward the nesting bound of 1,000 levels, below the block it filters; a flat
//! chain of operators adds no level (ledger 32).

use crate::test_helpers::{MAX_DEPTH, TOO_DEEP, check_depth_bound, parse_deep, walk_deep};

#[test]
fn test_nested_not_counts_each_level() {
    // The root block is level 1, the filter's bracket level 2; each `not (` opens a further bracket.
    check_depth_bound(|depth| {
        let nots = depth - 2;
        format!(
            "{{ q(func: uid(1)) @filter({}eq(a, 1){}) {{ a }} }}",
            "not (".repeat(nots),
            ")".repeat(nots)
        )
    });
}

#[test]
fn test_brackets_count_each_level() {
    // The root block is level 1, the filter's bracket level 2; depth - 2 further brackets around the function.
    let query = |depth: usize| {
        let brackets = depth - 2;
        format!(
            "{{ q(func: uid(1)) @filter({}eq(a, 1){}) {{ a }} }}",
            "(".repeat(brackets),
            ")".repeat(brackets)
        )
    };
    // Brackets alone build no tree levels: the filter is a single function below the block.
    assert_eq!(parse_deep(query(MAX_DEPTH)), Ok(2));
    assert_eq!(parse_deep(query(MAX_DEPTH + 1)), Err(TOO_DEEP.to_string()));
    assert_eq!(
        parse_deep(query(100 * MAX_DEPTH)),
        Err(TOO_DEEP.to_string())
    );
}

#[test]
fn test_a_flat_chain_of_or_adds_no_level() {
    // `a or b or c` is `(a or b) or c`: a tree as deep as the chain is long, every walk over it on a small stack.
    let terms = 5000;
    let query = format!(
        "{{ q(func: uid(1)) @filter(eq(a, 1){}) {{ a }} }}",
        " or eq(a, 1)".repeat(terms - 1)
    );
    assert_eq!(walk_deep(query), terms + 1);
}
