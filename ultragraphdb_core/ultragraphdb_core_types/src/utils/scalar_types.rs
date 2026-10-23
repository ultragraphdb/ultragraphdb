/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/scalar_types.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use crate::types::big_float::BigFloat;
use crate::types::date_time::DateTime;
use crate::types::date_time::date_time_parse::{
    LAYOUT_DATE_TIME, LAYOUT_RFC3339, LAYOUT_Y, LAYOUT_YM, LAYOUT_YMD, LAYOUT_YMD_ZONE, parse,
};
use crate::types::geometry::Geometry;
use crate::types::type_id::TypeId;
use crate::types::val::Val;
use crate::types::value::Value;

/// Dgraph's `BigFloatPrecision`: the bits of a bigfloat value.
pub const BIG_FLOAT_PRECISION: u32 = 200;

/// Dgraph's `ValueForType`: the zero value of a type; a type without one gives a nil value of `DefaultID`.
///
/// Go returns pointers to the zero values; this port returns the values. A geo value's zero is Go's nil `geom.T`,
/// which this port represents as an empty geometry collection.
pub fn value_for_type(id: TypeId) -> Val {
    let v = match id {
        TypeId::BINARY => Value::Bytes(Vec::new()),
        TypeId::INT => Value::Int(0),
        TypeId::FLOAT => Value::Float(0.0),
        TypeId::BOOL => Value::Bool(false),
        TypeId::DATETIME => Value::DateTime(DateTime::zero()),
        TypeId::BIGFLOAT => Value::BigFloat(BigFloat::zero(BIG_FLOAT_PRECISION)),
        TypeId::STRING | TypeId::DEFAULT | TypeId::PASSWORD => Value::Str(Vec::new()),
        TypeId::GEO => Value::Geo(Geometry::collection(Vec::new())),
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

/// Dgraph's `ParseTime`: a year, a year and month, a date, RFC 3339, `2006-01-02 15:04:05 -0700 MST`, or RFC 3339
/// without a zone, chosen by the text's length and shape, with Go's error texts.
pub fn parse_time(val: impl AsRef<[u8]>) -> Result<DateTime, TypesError> {
    let b = val.as_ref();
    match b.len() {
        4 => return parse(LAYOUT_Y, b),
        7 => return parse(LAYOUT_YM, b),
        10 => return parse(LAYOUT_YMD, b),
        _ => {}
    }
    if b.len() > LAYOUT_DATE_TIME.len()
        && b[10] == b'T'
        && (b[b.len() - 1] == b'Z' || b[b.len() - 3] == b':')
    {
        return parse(LAYOUT_RFC3339, b);
    }
    if let Ok(t) = parse(LAYOUT_YMD_ZONE, b) {
        return Ok(t);
    }
    parse(LAYOUT_DATE_TIME, b)
}
