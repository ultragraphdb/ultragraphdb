/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/options/options.go (the values of Options). Changed for ultragraphdb:
 * translated to Rust; Go's `any` as the closed set of values the option parsers produce.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::metric::Metric;

/// The value of one index option.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    /// Go's `int`, from the int parser.
    Int(i64),
    /// Go's `uint`, from the uint parser.
    Uint(u64),
    /// Go's `float64`, from the float64 and float32 parsers (Go's float32 parser returns a float64 too).
    Float(f64),
    /// Go's `bool`.
    Bool(bool),
    /// Go's `string`, from the string parser.
    Str(String),
    /// A vector index metric, Go's `hnsw.SimilarityType[float32]`.
    Metric(Metric),
}

impl OptionValue {
    /// Go's `%T` of the value, as Dgraph's messages print it.
    pub fn go_type(&self) -> &'static str {
        match self {
            OptionValue::Int(_) => "int",
            OptionValue::Uint(_) => "uint",
            OptionValue::Float(_) => "float64",
            OptionValue::Bool(_) => "bool",
            OptionValue::Str(_) => "string",
            OptionValue::Metric(_) => "hnsw.SimilarityType[float32]",
        }
    }
}
