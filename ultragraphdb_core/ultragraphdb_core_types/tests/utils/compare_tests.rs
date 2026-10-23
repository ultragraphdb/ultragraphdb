/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::{TypeId, Val, Value, compare_between, compare_vals};

fn int(i: i64) -> Val {
    Val::new(TypeId::INT, Value::Int(i))
}

#[test]
fn test_compare_vals() {
    assert!(compare_vals("ge", &int(3), &int(3)).unwrap());
    assert!(compare_vals("gt", &int(4), &int(3)).unwrap());
    assert!(!compare_vals("gt", &int(3), &int(3)).unwrap());
    assert!(compare_vals("le", &int(3), &int(3)).unwrap());
    assert!(compare_vals("lt", &int(2), &int(3)).unwrap());
    assert!(compare_vals("eq", &int(3), &int(3)).unwrap());
    // Values of different types never compare.
    let f = Val::new(TypeId::FLOAT, Value::Float(3.0));
    assert!(!compare_vals("ge", &int(3), &f).unwrap());
    assert!(!compare_vals("eq", &int(3), &f).unwrap());
}

#[test]
fn test_compare_between() {
    assert!(compare_between(&int(3), &int(1), &int(3)));
    assert!(!compare_between(&int(4), &int(1), &int(3)));
}

#[test]
fn test_compare_vals_unknown_op() {
    // Dgraph's x.Fatalf exits the process with this text.
    let err = compare_vals("ne", &int(1), &int(1)).unwrap_err();
    assert_eq!(err.to_string(), "Unknown ineqType ne");
}
