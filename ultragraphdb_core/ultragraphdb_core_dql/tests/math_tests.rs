/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The brackets and nested functions of a math expression count toward the nesting bound of 1,000 levels, below
//! the block that holds it; a flat chain of operators adds no level (ledger 32).

use crate::test_helpers::{MAX_DEPTH, TOO_DEEP, check_depth_bound, parse_deep, walk_deep};

/// The math block is level 2, its expression's root level 3.
fn math_query(expr: String) -> String {
    format!("{{ q(func: uid(1)) {{ x as count(uid) p : math({expr}) }} }}")
}

#[test]
fn test_nested_math_functions_count_each_level() {
    check_depth_bound(|depth| {
        let calls = depth - 3;
        math_query(format!("{}x{}", "exp(".repeat(calls), ")".repeat(calls)))
    });
}

#[test]
fn test_a_flat_chain_of_additions_adds_no_level() {
    // `x + x + x` is `(x + x) + x`: a tree as deep as the chain is long, every walk over it on a small stack.
    let terms = 5000;
    let query = math_query(format!("x{}", " + x".repeat(terms - 1)));
    assert_eq!(walk_deep(query), terms + 2);
}

#[test]
fn test_brackets_count_each_level() {
    // The expression inside `math(` is level 3; depth - 3 further brackets around the variable.
    let query = |depth: usize| {
        let brackets = depth - 3;
        math_query(format!("{}x{}", "(".repeat(brackets), ")".repeat(brackets)))
    };
    // Brackets alone build no tree levels: the expression is a single variable below the math block.
    assert_eq!(parse_deep(query(MAX_DEPTH)), Ok(3));
    assert_eq!(parse_deep(query(MAX_DEPTH + 1)), Err(TOO_DEEP.to_string()));
    assert_eq!(
        parse_deep(query(100 * MAX_DEPTH)),
        Err(TOO_DEEP.to_string())
    );
}

#[test]
fn test_nested_math_arguments_count_each_level() {
    // pow(x, pow(x, ...)): each call's arguments sit one level below it.
    check_depth_bound(|depth| {
        let calls = depth - 3;
        math_query(format!("{}x{}", "pow(x, ".repeat(calls), ")".repeat(calls)))
    });
}
