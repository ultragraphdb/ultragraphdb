/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/scalar_types.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

// Arithmetic on decoded text input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

use crate::errors::types_error::TypesError;
use crate::types::date_time::DateTime;
use crate::types::date_time::date_time_parse::parse;
use crate::types::type_id::TypeId;
use crate::types::val::Val;
use crate::types::value::Value;

/// Dgraph's `ValueForType`: the zero value of a type; a type without one gives a nil value of `DefaultID`.
///
/// Go returns pointers to the zero values; this port returns the values. A geo value's zero is the empty geometry
/// collection.
pub fn value_for_type(id: TypeId) -> Val {
    let v = match id {
        TypeId::BINARY => Value::Bytes(Vec::new()),
        TypeId::INT => Value::Int(0),
        TypeId::FLOAT => Value::Float(0.0),
        TypeId::BOOL => Value::Bool(false),
        TypeId::DATETIME => Value::DateTime(DateTime::zero()),
        TypeId::DECIMAL => Value::Decimal(oxsdatatypes::Decimal::from(0_i64)),
        TypeId::STRING | TypeId::DEFAULT | TypeId::PASSWORD => Value::Str(String::new()),
        TypeId::GEO => Value::Geo(ultragraphdb_core_geo::Geometry::empty()),
        TypeId::UID => Value::Uid(0),
        TypeId::VFLOAT => Value::VFloat(Vec::new()),
        _ => return Val::nil(TypeId::DEFAULT),
    };
    Val::new(id, v)
}

/// Dgraph's `GoodTimeZone`: whether an offset in seconds is within RFC 3339's ±23:59.
pub fn good_time_zone(offset: i64) -> bool {
    const BOUNDARY: i64 = 23 * 60 * 60 + 59 * 60;
    (-BOUNDARY..=BOUNDARY).contains(&offset)
}

/// Dgraph's `ParseTime`: the datetime a text holds, as a year, a year and month, a date, a date and time with `T`
/// between them and an optional zone, or a date and time with a numeric offset and a zone abbreviation.
///
/// # Errors
///
/// [`TypesError`] naming the text and what it breaks.
pub fn parse_time(val: impl AsRef<[u8]>) -> Result<DateTime, TypesError> {
    parse(val.as_ref())
}
