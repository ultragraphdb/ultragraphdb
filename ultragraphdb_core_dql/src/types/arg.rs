/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// An argument of a function: a value, `val(a)` as a value variable, or a `$name` query parameter.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Arg {
    pub(crate) value: String,
    pub(crate) is_value_var: bool,
    pub(crate) is_dql_var: bool,
}

impl Arg {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            ..Self::default()
        }
    }

    /// The value, or the variable's name.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Whether the argument is `val(a)`, e.g. `eq(name, val(a))`.
    pub fn is_value_var(&self) -> bool {
        self.is_value_var
    }

    /// Whether the argument is a `$name` query parameter, substituted from the request's variables.
    pub fn is_dql_var(&self) -> bool {
        self.is_dql_var
    }
}
