/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::{DateTime, TypeId, Val, Value, compare_between, compare_vals};

fn int(i: i64) -> Val {
    Val::new(TypeId::INT, Value::Int(i))
}

fn float(f: f64) -> Val {
    Val::new(TypeId::FLOAT, Value::Float(f))
}

fn string(s: &str) -> Val {
    Val::new(TypeId::STRING, Value::str(s))
}

fn boolean(b: bool) -> Val {
    Val::new(TypeId::BOOL, Value::Bool(b))
}

fn uid(u: u64) -> Val {
    Val::new(TypeId::UID, Value::Uid(u))
}

fn year(y: i64) -> Val {
    Val::new(
        TypeId::DATETIME,
        Value::DateTime(DateTime::utc(y, 1, 1, 0, 0, 0, 0)),
    )
}

/// Dgraph's `CompareVals` for `ge`, `gt`, `le`, `lt` and `eq`, in that order, as Go v25.5.0 answers them. `ge` and
/// `le` negate `Less`, so NaN is `ge` and `le` everything; uid compares through `Less` but not `Equal`, bool
/// through `Equal` but not `Less`; values of different types never compare.
#[test]
fn test_compare_vals() {
    let cases: Vec<(&str, Val, Val, [bool; 5])> = vec![
        ("3 3", int(3), int(3), [true, false, true, false, true]),
        ("2 3", int(2), int(3), [false, false, true, true, false]),
        ("4 3", int(4), int(3), [true, true, false, false, false]),
        (
            "min max",
            int(i64::MIN),
            int(i64::MAX),
            [false, false, true, true, false],
        ),
        (
            "-0 0",
            float(-0.0),
            float(0.0),
            [true, false, true, false, true],
        ),
        (
            "NaN 1",
            float(f64::NAN),
            float(1.0),
            [true, false, true, false, false],
        ),
        (
            "NaN NaN",
            float(f64::NAN),
            float(f64::NAN),
            [true, false, true, false, false],
        ),
        (
            "1.5 2.5",
            float(1.5),
            float(2.5),
            [false, false, true, true, false],
        ),
        (
            "a b",
            string("a"),
            string("b"),
            [false, false, true, true, false],
        ),
        (
            "b a",
            string("b"),
            string("a"),
            [true, true, false, false, false],
        ),
        (
            "empty empty",
            string(""),
            string(""),
            [true, false, true, false, true],
        ),
        (
            "true true",
            boolean(true),
            boolean(true),
            [false, false, false, false, true],
        ),
        (
            "true false",
            boolean(true),
            boolean(false),
            [false, false, false, false, false],
        ),
        ("uid 1 2", uid(1), uid(2), [false, false, true, true, false]),
        ("uid 2 2", uid(2), uid(2), [true, false, true, false, false]),
        (
            "2000 2001",
            year(2000),
            year(2001),
            [false, false, true, true, false],
        ),
        (
            "2001 2001",
            year(2001),
            year(2001),
            [true, false, true, false, true],
        ),
        (
            "int float",
            int(3),
            float(3.0),
            [false, false, false, false, false],
        ),
        (
            "string int",
            string("1"),
            int(1),
            [false, false, false, false, false],
        ),
    ];
    for (name, a, b, want) in cases {
        let got = ["ge", "gt", "le", "lt", "eq"].map(|op| compare_vals(op, &a, &b).unwrap());
        assert_eq!(got, want, "{name}");
    }
}

/// Dgraph's `CompareBetween`, as Go v25.5.0 answers it: both ends inclusive, an inverted range holds nothing.
#[test]
fn test_compare_between() {
    let cases = [
        ("2 in 1..3", int(2), int(1), int(3), true),
        ("1 in 1..3", int(1), int(1), int(3), true),
        ("3 in 1..3", int(3), int(1), int(3), true),
        ("0 in 1..3", int(0), int(1), int(3), false),
        ("4 in 1..3", int(4), int(1), int(3), false),
        ("2 in 3..1", int(2), int(3), int(1), false),
        ("2 in 2..2", int(2), int(2), int(2), true),
        ("float in int range", float(2.0), int(1), int(3), false),
        ("b in a..c", string("b"), string("a"), string("c"), true),
        ("bool", boolean(true), boolean(false), boolean(true), false),
    ];
    for (name, dst, lo, hi, want) in cases {
        assert_eq!(compare_between(&dst, &lo, &hi), want, "{name}");
    }
}

#[test]
fn test_compare_vals_unknown_op() {
    // Dgraph's x.Fatalf exits the process with this text.
    for op in ["ne", "", "GE", "eq "] {
        let err = compare_vals(op, &int(1), &int(1)).unwrap_err();
        assert_eq!(err.to_string(), format!("Unknown ineqType {op}"));
    }
}
