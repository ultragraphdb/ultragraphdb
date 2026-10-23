/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Equality of the trees, compared without recursion, looks at every node and every field (ledger 32).

use ultragraphdb_core_dql::{ParseResult, Request, parse};

fn tree(q: &str) -> ParseResult {
    parse(&Request::new(q)).expect("parses")
}

#[test]
fn test_trees_differing_deep_down_are_unequal() {
    let a = tree("{ q(func: uid(1)) { a { b { c @filter(eq(d, 1) or eq(d, 2)) } } } }");
    let b = tree("{ q(func: uid(1)) { a { b { c @filter(eq(d, 1) or eq(d, 3)) } } } }");
    assert!(a == tree("{ q(func: uid(1)) { a { b { c @filter(eq(d, 1) or eq(d, 2)) } } } }"));
    assert!(a != b);
    assert!(a.query()[0].children()[0] != b.query()[0].children()[0]);
}

#[test]
fn test_trees_differing_in_children_are_unequal() {
    let a = tree("{ q(func: uid(1)) { a { b c } } }");
    let b = tree("{ q(func: uid(1)) { a { b } } }");
    assert!(a != b);
    let m = tree("{ q(func: uid(1)) { x as count(uid) p : math(x + 1) } }");
    let n = tree("{ q(func: uid(1)) { x as count(uid) p : math(x + 2) } }");
    let math = |r: &ParseResult| r.query()[0].children()[1].math_exp().cloned();
    assert!(math(&m) != math(&n));
    assert!(math(&m) == math(&m));
}
