/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/compare.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::val::Val;
use crate::utils::sort::{equal, less};

/// Dgraph's `CompareVals`: whether `arg1 <op> arg2` for `op` one of `ge`, `gt`, `le`, `lt`, `eq`; false when the
/// values cannot be compared.
///
/// # Panics
///
/// On any other `op`, which Dgraph treats as fatal ("should have been checked at query level").
pub fn compare_vals(op: &str, arg1: &Val, arg2: &Val) -> bool {
    match op {
        "ge" => less(arg1, arg2).is_ok_and(|b| !b),
        "gt" => less(arg2, arg1).unwrap_or(false),
        "le" => less(arg2, arg1).is_ok_and(|b| !b),
        "lt" => less(arg1, arg2).unwrap_or(false),
        "eq" => equal(arg1, arg2).unwrap_or(false),
        _ => panic!("Unknown ineqType {op}"),
    }
}

/// Dgraph's `CompareBetween`: whether `dst` lies between `val1` and `val2`, both inclusive.
pub fn compare_between(dst: &Val, val1: &Val, val2: &Val) -> bool {
    compare_vals("ge", dst, val1) && compare_vals("le", dst, val2)
}
