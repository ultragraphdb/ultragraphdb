/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Every failing variable and argument is reported, in the order of their names (ledger 12).

use ultragraphdb_core_dql::{Request, parse};

use crate::test_helpers::vars;

fn error(r: &Request) -> String {
    parse(r).expect_err("the query fails").to_string()
}

#[test]
fn test_every_failing_variable_is_reported_in_name_order() {
    let q = "query test($d: blah, $b: int!, $a: int, $c: float) { q(func: uid(1)) { name } }";
    let r = vars(q, &[("$e", "1"), ("$d", "v"), ("$c", "1.5"), ("$a", "x")]);
    assert_eq!(
        error(&r),
        [
            r#"Expected an int but got x: strconv.ParseInt: parsing "x": invalid syntax"#,
            "Variable $b should be initialised",
            r#"Type "blah" not supported"#,
            "Type of variable $e not specified",
        ]
        .join("\n")
    );
}

#[test]
fn test_one_failing_variable_is_reported_alone() {
    let q = "query test($a: int) { q(func: uid(1)) { name } }";
    let r = vars(q, &[("$a", "3"), ("$b", "5")]);
    assert_eq!(error(&r), "Type of variable $b not specified");
}

#[test]
fn test_every_undefined_argument_is_reported_in_name_order() {
    let q = "{ q(func: uid(1)) { friends(offset: $a, first: $b) { name } } }";
    assert_eq!(
        error(&Request::new(q)),
        "Variable not defined $b\nVariable not defined $a"
    );
}
