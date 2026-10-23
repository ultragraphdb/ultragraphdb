/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/compare.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use crate::types::val::Val;
use crate::utils::sort::{equal, less};

/// Dgraph's `CompareVals`: whether `arg1 <op> arg2` for `op` one of `ge`, `gt`, `le`, `lt`, `eq`; false when the
/// values cannot be compared.
///
/// # Errors
///
/// `Unknown ineqType <op>` for any other `op`, the text Dgraph's `x.Fatalf` exits with ("should have been checked
/// at query level").
pub fn compare_vals(op: &str, arg1: &Val, arg2: &Val) -> Result<bool, TypesError> {
    Ok(match op {
        "ge" => ge(arg1, arg2),
        "gt" => less(arg2, arg1).unwrap_or(false),
        "le" => ge(arg2, arg1),
        "lt" => less(arg1, arg2).unwrap_or(false),
        "eq" => equal(arg1, arg2).unwrap_or(false),
        _ => return Err(TypesError::compare(format!("Unknown ineqType {op}"))),
    })
}

/// Dgraph's `CompareBetween`: whether `dst` lies between `val1` and `val2`, both inclusive.
pub fn compare_between(dst: &Val, val1: &Val, val2: &Val) -> bool {
    ge(dst, val1) && ge(val2, dst)
}

/// `a >= b`, false when the values cannot be compared.
fn ge(a: &Val, b: &Val) -> bool {
    less(a, b).is_ok_and(|b| !b)
}
