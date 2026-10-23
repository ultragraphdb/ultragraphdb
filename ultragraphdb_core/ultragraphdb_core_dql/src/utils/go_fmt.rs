/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's `%v` of the Dgraph type dql prints in its messages; `%v` of a `[]string` is
//! `ultragraphdb_common_utils::go_fmt::string_slice`.

use crate::types::arg::Arg;

/// `%v` of a `[]Arg`: `[{value false false} ...]`.
pub(crate) fn arg_slice(v: &[Arg]) -> String {
    let args: Vec<String> = v
        .iter()
        .map(|a| format!("{{{} {} {}}}", a.value(), a.is_value_var(), a.is_dql_var()))
        .collect();
    format!("[{}]", args.join(" "))
}
