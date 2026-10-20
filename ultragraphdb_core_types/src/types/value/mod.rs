/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/scalar_types.go (Val.Value). Changed for ultragraphdb: translated to Rust; the
 * Go interface{} values Dgraph stores in a Val, as one enum.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::big_float::BigFloat;
use crate::types::date_time::DateTime;
use crate::types::geometry::Geometry;

/// The value of a [`Val`](crate::Val): raw bytes before conversion, or a converted Go value.
///
/// A Go string holds any bytes, not only UTF-8, and Dgraph converts binary values to strings and back unchanged,
/// so [`Value::Str`] holds bytes too; [`Value::str`] builds one from text.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `[]byte`: a binary value, or the unconverted form of any type.
    Bytes(Vec<u8>),
    /// `string`: string, default and password values, as Go's bytes.
    Str(Vec<u8>),
    /// `int64`.
    Int(i64),
    /// `float64`.
    Float(f64),
    /// `bool`.
    Bool(bool),
    /// `time.Time`.
    DateTime(DateTime),
    /// `geom.T`.
    Geo(Geometry),
    /// `uint64`.
    Uid(u64),
    /// `big.Float`.
    BigFloat(BigFloat),
    /// `[]float32`.
    VFloat(Vec<f32>),
}

impl Value {
    /// A string value from text.
    pub fn str(s: impl AsRef<str>) -> Value {
        Value::Str(s.as_ref().as_bytes().to_vec())
    }

    /// Go's `%T` of the value, as Dgraph's messages print it.
    pub fn go_type(&self) -> &'static str {
        match self {
            Value::Bytes(_) => "[]uint8",
            Value::Str(_) => "string",
            Value::Int(_) => "int64",
            Value::Float(_) => "float64",
            Value::Bool(_) => "bool",
            Value::DateTime(_) => "time.Time",
            Value::Geo(g) => g.kind().go_type(),
            Value::Uid(_) => "uint64",
            Value::BigFloat(_) => "big.Float",
            Value::VFloat(_) => "[]float32",
        }
    }
}
