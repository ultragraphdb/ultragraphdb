/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/constraints/constraints.go (Simple) and tok/options/options.go (GetOpt's type
 * assertion). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::option_value::OptionValue;

/// A simple option type `Options::get_opt` reads: Go's `constraints.Simple`.
pub trait OptionType: Sized {
    /// Go's `%T` of the type.
    const GO_TYPE: &'static str;

    /// The value as this type, `None` when it holds another.
    fn from_option(v: &OptionValue) -> Option<Self>;
}

impl OptionType for i64 {
    const GO_TYPE: &'static str = "int";

    fn from_option(v: &OptionValue) -> Option<Self> {
        match v {
            OptionValue::Int(i) => Some(*i),
            _ => None,
        }
    }
}

impl OptionType for u64 {
    const GO_TYPE: &'static str = "uint";

    fn from_option(v: &OptionValue) -> Option<Self> {
        match v {
            OptionValue::Uint(i) => Some(*i),
            _ => None,
        }
    }
}

impl OptionType for f64 {
    const GO_TYPE: &'static str = "float64";

    fn from_option(v: &OptionValue) -> Option<Self> {
        match v {
            OptionValue::Float(f) => Some(*f),
            _ => None,
        }
    }
}

impl OptionType for bool {
    const GO_TYPE: &'static str = "bool";

    fn from_option(v: &OptionValue) -> Option<Self> {
        match v {
            OptionValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

impl OptionType for String {
    const GO_TYPE: &'static str = "string";

    fn from_option(v: &OptionValue) -> Option<Self> {
        match v {
            OptionValue::Str(s) => Some(s.clone()),
            _ => None,
        }
    }
}
