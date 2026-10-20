/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's `%v` of the slices Dgraph's dql package prints in its messages.

use crate::types::arg::Arg;

/// `%v` of a `[]string`: `[a b c]`.
pub(crate) fn string_slice(v: &[String]) -> String {
    format!("[{}]", v.join(" "))
}

/// `%v` of a `[]Arg`: `[{value false false} ...]`.
pub(crate) fn arg_slice(v: &[Arg]) -> String {
    let args: Vec<String> = v
        .iter()
        .map(|a| format!("{{{} {} {}}}", a.value(), a.is_value_var(), a.is_dql_var()))
        .collect();
    format!("[{}]", args.join(" "))
}
