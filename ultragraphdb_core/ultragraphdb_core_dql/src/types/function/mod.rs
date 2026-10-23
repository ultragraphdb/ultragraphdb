/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::arg::Arg;
use crate::types::var_context::VarContext;

/// A DQL function: `eq(name, "x")`, `uid(a, 0x1)`, `gt(count(friends), 0)`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Function {
    pub(crate) attr: String,
    pub(crate) lang: String,
    pub(crate) name: String,
    pub(crate) args: Vec<Arg>,
    pub(crate) uid: Vec<u64>,
    pub(crate) needs_var: Vec<VarContext>,
    pub(crate) is_count: bool,
    pub(crate) is_value_var: bool,
    pub(crate) is_len_var: bool,
}

impl Function {
    /// The attribute the function applies to.
    pub fn attr(&self) -> &str {
        &self.attr
    }

    /// The language of the attribute's value.
    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// The function's name, lowercased.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn args(&self) -> &[Arg] {
        &self.args
    }

    /// The uids `uid(0x1, 0x2)` names.
    pub fn uid(&self) -> &[u64] {
        &self.uid
    }

    /// The variables the function requires.
    pub fn needs_var(&self) -> &[VarContext] {
        &self.needs_var
    }

    /// Whether the attribute is `count(friends)`, as in `gt(count(friends), 0)`.
    pub fn is_count(&self) -> bool {
        self.is_count
    }

    /// Whether the attribute is `val(s)`, as in `eq(val(s), 5)`.
    pub fn is_value_var(&self) -> bool {
        self.is_value_var
    }

    /// Whether the attribute is `len(s)`, as in `eq(len(s), 5)`.
    pub fn is_len_var(&self) -> bool {
        self.is_len_var
    }

    /// Whether the function is an aggregation: `min`, `max`, `sum` or `avg`.
    pub fn is_aggregator(&self) -> bool {
        is_aggregator(&self.name)
    }

    /// Whether the function is `checkpwd`.
    pub fn is_password_verifier(&self) -> bool {
        self.name == "checkpwd"
    }
}

/// Whether the function name is an aggregation function.
pub(crate) fn is_aggregator(fname: &str) -> bool {
    matches!(fname, "min" | "max" | "sum" | "avg")
}
