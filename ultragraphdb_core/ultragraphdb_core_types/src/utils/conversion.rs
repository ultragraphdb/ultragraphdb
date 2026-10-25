/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/conversion.go. Changed for ultragraphdb: translated to Rust; a float outside int64
 * is refused as an int and as a datetime, bytes that are not UTF-8 are refused as a string, and `toBinary` returns
 * an error where Go panics.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_dgraph::api;

use crate::errors::types_error::TypesError;
use crate::types::big_float::BigFloat;
use crate::types::date_time::DateTime;
use crate::types::geometry::Geometry;
use crate::types::type_id::TypeId;
use crate::types::val::Val;
use crate::types::value::Value;
use crate::utils::json_float::json_float;
use crate::utils::password::encrypt;
use crate::utils::scalar_types::{BIG_FLOAT_PRECISION, parse_time, value_for_type};
use crate::utils::value::{bytes_as_float_array, float_array_as_bytes, float_array_as_string};
use ultragraphdb_common_utils::{go_fmt, go_strconv};

const NANO_SECONDS_IN_SEC: i64 = 1_000_000_000;

/// Go's `%v` of the values `ObjectValue` reports. Datetimes, geometries and big floats print in their text forms,
/// where Go prints its struct.
fn go_value(v: &Value) -> String {
    match v {
        Value::Bytes(b) => go_fmt::byte_slice(b),
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => go_strconv::format_float(*f, b'g', 64),
        Value::Bool(b) => go_strconv::format_bool(*b).into(),
        Value::DateTime(t) => t.marshal_text().unwrap_or_default(),
        Value::Geo(g) => g.to_geojson().unwrap_or_default(),
        Value::Uid(u) => u.to_string(),
        Value::BigFloat(b) => b.text_g(10),
        Value::VFloat(v) => {
            let items: Vec<String> = v
                .iter()
                .map(|f| go_strconv::format_float(f64::from(*f), b'g', 32))
                .collect();
            format!("[{}]", items.join(" "))
        }
    }
}

fn cant_convert(from: TypeId, to: TypeId) -> TypesError {
    TypesError::conversion(format!(
        "Cannot convert {} to type {}",
        from.name(),
        to.name()
    ))
}

fn cannot_convert_to_vfloat(s: &str) -> TypesError {
    TypesError::conversion(format!("cannot convert {s} to vfloat"))
}

fn lossy(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

/// The bytes as the string value of a type named `type_name`, refused when they are not UTF-8: Go keeps them as they
/// are, and text analysis then reads past them or drops what follows (ledger 17, 18, 31).
fn utf8(data: &[u8], type_name: &str) -> Result<String, TypesError> {
    String::from_utf8(data.to_vec()).map_err(|_| {
        TypesError::conversion(format!("Value is not valid UTF-8 for type {type_name}"))
    })
}

/// 2^63, the least float above int64.
const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;

/// Whether a float lies in int64, from -2^63 to below 2^63. NaN does not.
fn in_int64(f: f64) -> bool {
    (-TWO_POW_63..TWO_POW_63).contains(&f)
}

fn float_out_of_range() -> TypesError {
    TypesError::conversion("Float out of int64 range")
}

/// The little-endian int64 in the first eight bytes, Dgraph's `invalid data for int64` for fewer.
fn le_i64(data: &[u8]) -> Result<i64, TypesError> {
    match data.first_chunk::<8>() {
        Some(b) => Ok(i64::from_le_bytes(*b)),
        None => Err(TypesError::conversion(format!(
            "invalid data for int64 {}",
            go_fmt::byte_slice(data)
        ))),
    }
}

/// The little-endian float64 in the first eight bytes, Dgraph's `invalid data for float` for fewer.
fn le_f64(data: &[u8]) -> Result<f64, TypesError> {
    match data.first_chunk::<8>() {
        Some(b) => Ok(f64::from_bits(u64::from_le_bytes(*b))),
        None => Err(TypesError::conversion(format!(
            "invalid data for float {}",
            go_fmt::byte_slice(data)
        ))),
    }
}

fn parse_big_float(data: &[u8]) -> Result<BigFloat, TypesError> {
    BigFloat::unmarshal_text(data, BIG_FLOAT_PRECISION).map_err(TypesError::big_float)
}

fn big_float_from_f64(f: f64, prec: u32) -> Result<BigFloat, TypesError> {
    BigFloat::from_f64(f, prec).ok_or_else(|| TypesError::conversion("Float.SetFloat64(NaN)"))
}

/// Go's `time.Unix(secs, nsecs).UTC()` from a float of seconds, as Dgraph splits it. Go converts floats outside int64
/// to int64 as its platform does, so this port refuses them, NaN and the infinities included, before `secs` is taken
/// (ledger 10).
fn date_time_from_float_secs(secs: i64, float_secs: f64) -> DateTime {
    let frac = float_secs - secs as f64;
    let nsecs = (frac * NANO_SECONDS_IN_SEC as f64) as i64;
    DateTime::from_unix(secs, nsecs)
}

/// Whether a big float lies in int64, from -2^63 to below 2^63.
fn big_float_in_int64(t: &BigFloat) -> bool {
    let bound = |f: f64| BigFloat::from_f64(f, 64);
    match (bound(-TWO_POW_63), bound(TWO_POW_63)) {
        (Some(lo), Some(hi)) => {
            t.compare(&lo) != std::cmp::Ordering::Less && t.compare(&hi) == std::cmp::Ordering::Less
        }
        // from_f64 refuses NaN alone, and both bounds are finite.
        _ => false,
    }
}

/// Dgraph's `ParseVFloat`: `[a, b, ...]` or `[a b ...]` of float32 values, or an empty string.
pub fn parse_vfloat(s: impl AsRef<[u8]>) -> Result<Vec<f32>, TypesError> {
    let s = lossy(s.as_ref()).replace(['\n', '\t'], " ");
    let s = s.trim();
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let Some(trimmed_pre) = s.strip_prefix('[') else {
        return Err(cannot_convert_to_vfloat(s));
    };
    let trimmed = trimmed_pre.trim_end_matches(']');
    if trimmed.len() == trimmed_pre.len() {
        return Err(cannot_convert_to_vfloat(s));
    }
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.contains(',') {
        return trimmed
            .split(',')
            .map(|v| {
                go_strconv::parse_float(v.trim(), 32)
                    .map(|f| f as f32)
                    .map_err(|_| cannot_convert_to_vfloat(s))
            })
            .collect();
    }
    trimmed
        .split(' ')
        .filter(|v| !v.is_empty())
        .map(|v| {
            go_strconv::parse_float(v, 32)
                .map(|f| f as f32)
                .map_err(|_| cannot_convert_to_vfloat(s))
        })
        .collect()
}

/// Dgraph's `Convert`: the binary or text form of a value (`from` must hold [`Value::Bytes`]) converted to a
/// type, with Dgraph's rules and error texts.
pub fn convert(from: &Val, to_id: TypeId) -> Result<Val, TypesError> {
    let Some(Value::Bytes(data)) = from.value() else {
        return Err(TypesError::conversion(format!(
            "invalid data to convert to {}",
            to_id.name()
        )));
    };
    let from_id = from.tid();
    let ok = |v: Value| Ok(Val::new(to_id, v));
    match from_id {
        TypeId::BINARY => match to_id {
            TypeId::BINARY => ok(Value::Bytes(data.clone())),
            TypeId::STRING | TypeId::DEFAULT => ok(Value::Str(utf8(data, "string")?)),
            TypeId::INT => ok(Value::Int(le_i64(data)?)),
            TypeId::FLOAT => ok(Value::Float(le_f64(data)?)),
            TypeId::BIGFLOAT => ok(Value::BigFloat(parse_big_float(data)?)),
            TypeId::BOOL => match data.first() {
                None | Some(0) => ok(Value::Bool(false)),
                Some(1) => ok(Value::Bool(true)),
                Some(b) => Err(TypesError::conversion(format!(
                    "invalid value for bool {b}"
                ))),
            },
            TypeId::DATETIME => ok(Value::DateTime(DateTime::unmarshal_binary(data)?)),
            TypeId::GEO => ok(Value::Geo(Geometry::unmarshal_wkb(data)?)),
            TypeId::PASSWORD => ok(Value::Str(utf8(data, "password")?)),
            TypeId::VFLOAT => {
                if data.len() % 4 != 0 {
                    return Err(TypesError::conversion(format!(
                        "invalid data for vector of floats: {}",
                        go_fmt::byte_slice(data)
                    )));
                }
                ok(Value::VFloat(bytes_as_float_array(data)))
            }
            _ => Err(cant_convert(from_id, to_id)),
        },
        TypeId::STRING | TypeId::DEFAULT => match to_id {
            TypeId::BINARY => ok(Value::Bytes(data.clone())),
            TypeId::INT => ok(Value::Int(go_strconv::parse_int(data, 10)?)),
            TypeId::FLOAT => {
                let f = go_strconv::parse_float(data, 64)?;
                if f.is_nan() {
                    return Err(TypesError::conversion("Got invalid value: NaN"));
                }
                ok(Value::Float(f))
            }
            TypeId::BIGFLOAT => ok(Value::BigFloat(parse_big_float(data)?)),
            TypeId::STRING | TypeId::DEFAULT => ok(Value::Str(utf8(data, "string")?)),
            TypeId::BOOL => ok(Value::Bool(go_strconv::parse_bool(data)?)),
            TypeId::DATETIME => ok(Value::DateTime(parse_time(data)?)),
            TypeId::GEO => {
                let text: Vec<u8> = data
                    .iter()
                    .map(|&c| if c == b'\'' { b'"' } else { c })
                    .collect();
                match Geometry::from_geojson(&text) {
                    Ok(Some(g)) => ok(Value::Geo(g)),
                    Ok(None) => Ok(Val::nil(to_id)),
                    Err(e) => Err(e.wrap(&format!(
                        "Error while unmarshalling: [{}] as geojson",
                        lossy(data)
                    ))),
                }
            }
            TypeId::PASSWORD => ok(Value::Str(encrypt(data)?)),
            TypeId::VFLOAT => ok(Value::VFloat(parse_vfloat(data)?)),
            _ => Err(cant_convert(from_id, to_id)),
        },
        TypeId::INT => {
            let vc = le_i64(data)?;
            match to_id {
                TypeId::INT => ok(Value::Int(vc)),
                TypeId::BINARY => ok(Value::Bytes(vc.to_le_bytes().to_vec())),
                TypeId::FLOAT => ok(Value::Float(vc as f64)),
                TypeId::BIGFLOAT => {
                    ok(Value::BigFloat(BigFloat::from_i64(vc, BIG_FLOAT_PRECISION)))
                }
                TypeId::BOOL => ok(Value::Bool(vc != 0)),
                TypeId::STRING | TypeId::DEFAULT => ok(Value::str(vc.to_string())),
                TypeId::DATETIME => ok(Value::DateTime(DateTime::from_unix(vc, 0))),
                TypeId::VFLOAT => ok(Value::VFloat(vec![vc as f32])),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::BIGFLOAT => {
            let t = parse_big_float(data)?;
            match to_id {
                TypeId::BIGFLOAT => ok(Value::BigFloat(t)),
                TypeId::FLOAT => ok(Value::Float(t.to_f64())),
                TypeId::BINARY => ok(Value::Bytes(t.marshal_text().into_bytes())),
                TypeId::INT => ok(Value::Int(t.to_i64())),
                TypeId::BOOL => ok(Value::Bool(!t.is_zero())),
                TypeId::STRING | TypeId::DEFAULT => ok(Value::str(t.text_g(10))),
                TypeId::DATETIME => {
                    if !big_float_in_int64(&t) {
                        return Err(float_out_of_range());
                    }
                    ok(Value::DateTime(date_time_from_float_secs(
                        t.to_i64(),
                        t.to_f64(),
                    )))
                }
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::FLOAT => {
            let vc = le_f64(data)?;
            match to_id {
                TypeId::FLOAT => ok(Value::Float(vc)),
                TypeId::BIGFLOAT => ok(Value::BigFloat(big_float_from_f64(
                    vc,
                    BIG_FLOAT_PRECISION,
                )?)),
                TypeId::BINARY => ok(Value::Bytes(vc.to_bits().to_le_bytes().to_vec())),
                TypeId::INT => {
                    // Dgraph's guard lets 2^63 through, which its platforms convert differently (ledger 10).
                    if !in_int64(vc) {
                        return Err(float_out_of_range());
                    }
                    ok(Value::Int(vc as i64))
                }
                TypeId::BOOL => ok(Value::Bool(vc != 0.0)),
                TypeId::STRING | TypeId::DEFAULT => {
                    ok(Value::str(go_strconv::format_float(vc, b'G', 64)))
                }
                TypeId::DATETIME => {
                    if !in_int64(vc) {
                        return Err(float_out_of_range());
                    }
                    ok(Value::DateTime(date_time_from_float_secs(vc as i64, vc)))
                }
                TypeId::VFLOAT => ok(Value::VFloat(vec![vc as f32])),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::BOOL => {
            if data.is_empty() || data[0] > 1 {
                return Err(TypesError::conversion(format!(
                    "invalid value for bool {}",
                    go_fmt::byte_slice(data)
                )));
            }
            let vc = data[0] == 1;
            match to_id {
                TypeId::BOOL => ok(Value::Bool(vc)),
                TypeId::BINARY => ok(Value::Bytes(vec![u8::from(vc)])),
                TypeId::INT => ok(Value::Int(i64::from(vc))),
                TypeId::FLOAT => ok(Value::Float(if vc { 1.0 } else { 0.0 })),
                TypeId::VFLOAT => ok(Value::VFloat(vec![if vc { 1.0 } else { 0.0 }])),
                TypeId::BIGFLOAT => ok(Value::BigFloat(BigFloat::from_i64(
                    i64::from(vc),
                    BIG_FLOAT_PRECISION,
                ))),
                TypeId::STRING | TypeId::DEFAULT => ok(Value::str(go_strconv::format_bool(vc))),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::DATETIME => {
            let t = DateTime::unmarshal_binary(data)?;
            match to_id {
                TypeId::DATETIME => ok(Value::DateTime(t)),
                TypeId::BINARY => ok(Value::Bytes(t.marshal_binary()?)),
                TypeId::STRING | TypeId::DEFAULT => ok(Value::str(t.marshal_text()?)),
                TypeId::INT => ok(Value::Int(t.unix())),
                TypeId::FLOAT => ok(Value::Float(
                    t.unix_nano() as f64 / NANO_SECONDS_IN_SEC as f64,
                )),
                TypeId::BIGFLOAT => {
                    let x = big_float_from_f64(NANO_SECONDS_IN_SEC as f64, 53)?;
                    let y = big_float_from_f64(t.unix_nano() as f64, 53)?;
                    // x is 1e9, so the quotient is never NaN.
                    let q = y.quo(&x, 53).ok_or_else(|| {
                        TypesError::big_float("division of zero by zero or infinity by infinity")
                    })?;
                    ok(Value::BigFloat(q))
                }
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::GEO => {
            let g = Geometry::unmarshal_wkb(data)?;
            match to_id {
                TypeId::GEO => ok(Value::Geo(g)),
                TypeId::BINARY => ok(Value::Bytes(g.marshal_wkb()?)),
                TypeId::STRING | TypeId::DEFAULT => match g.to_geojson() {
                    Some(s) => ok(Value::str(s.replace('"', "'"))),
                    None => Ok(Val::nil(to_id)),
                },
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::PASSWORD => match to_id {
            TypeId::BINARY => ok(Value::Bytes(data.clone())),
            TypeId::STRING => ok(Value::Str(utf8(data, "string")?)),
            TypeId::PASSWORD => ok(Value::Str(utf8(data, "password")?)),
            _ => Err(cant_convert(from_id, to_id)),
        },
        TypeId::VFLOAT => match to_id {
            TypeId::BINARY => ok(Value::Bytes(data.clone())),
            TypeId::VFLOAT => ok(Value::VFloat(bytes_as_float_array(data))),
            TypeId::STRING => ok(Value::str(float_array_as_string(&bytes_as_float_array(
                data,
            )))),
            _ => Err(cant_convert(from_id, to_id)),
        },
        _ => Err(cant_convert(from_id, to_id)),
    }
}

/// Go's runtime panic on a failed type assertion, which `Marshal` hits when a value's Go type does not match its
/// type identifier; this port returns it as an error.
fn assertion(v: &Value, want: &str) -> TypesError {
    TypesError::conversion(format!(
        "interface conversion: interface {{}} is {}, not {want}",
        v.go_type()
    ))
}

/// Dgraph's `Marshal`: a converted value written as the binary or string form of `to_id`.
pub fn marshal(from: &Val, to_id: TypeId) -> Result<Val, TypesError> {
    let Some(val) = from.value() else {
        return Ok(value_for_type(to_id));
    };
    let from_id = from.tid();
    let ok = |v: Value| Ok(Val::new(to_id, v));
    let is_str = to_id == TypeId::STRING || to_id == TypeId::DEFAULT;
    match from_id {
        TypeId::BINARY => {
            let Value::Bytes(vc) = val else {
                return Err(assertion(val, "[]uint8"));
            };
            match to_id {
                _ if is_str => ok(Value::Str(utf8(vc, "string")?)),
                TypeId::BINARY => ok(Value::Bytes(vc.clone())),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::STRING | TypeId::DEFAULT => {
            let Value::Str(vc) = val else {
                return Err(assertion(val, "string"));
            };
            match to_id {
                _ if is_str => ok(Value::Str(vc.clone())),
                TypeId::BINARY => ok(Value::Bytes(vc.as_bytes().to_vec())),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::INT => {
            let Value::Int(vc) = val else {
                return Err(assertion(val, "int64"));
            };
            match to_id {
                _ if is_str => ok(Value::str(vc.to_string())),
                TypeId::BINARY => ok(Value::Bytes(vc.to_le_bytes().to_vec())),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::FLOAT => {
            let Value::Float(vc) = val else {
                return Err(assertion(val, "float64"));
            };
            match to_id {
                _ if is_str => ok(Value::str(go_strconv::format_float(*vc, b'G', 64))),
                TypeId::BINARY => ok(Value::Bytes(vc.to_bits().to_le_bytes().to_vec())),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::BIGFLOAT => {
            let Value::BigFloat(vc) = val else {
                return Err(assertion(val, "big.Float"));
            };
            match to_id {
                _ if is_str => ok(Value::str(vc.text_g(10))),
                TypeId::BINARY => ok(Value::Bytes(vc.marshal_text().into_bytes())),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::BOOL => {
            let Value::Bool(vc) = val else {
                return Err(assertion(val, "bool"));
            };
            match to_id {
                _ if is_str => ok(Value::str(go_strconv::format_bool(*vc))),
                TypeId::BINARY => ok(Value::Bytes(vec![u8::from(*vc)])),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::DATETIME => {
            let Value::DateTime(vc) = val else {
                return Err(assertion(val, "time.Time"));
            };
            match to_id {
                _ if is_str => ok(Value::str(vc.marshal_text()?)),
                TypeId::BINARY => ok(Value::Bytes(vc.marshal_binary()?)),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::GEO => {
            let Value::Geo(vc) = val else {
                return Err(TypesError::conversion("Expected a Geo type"));
            };
            match to_id {
                TypeId::BINARY => ok(Value::Bytes(vc.marshal_wkb()?)),
                // Go returns no error and leaves the target untouched when GeoJSON encoding fails.
                _ if is_str => match vc.to_geojson() {
                    Some(s) => ok(Value::str(s.replace('"', "'"))),
                    None => Ok(value_for_type(to_id)),
                },
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::PASSWORD => {
            let Value::Str(vc) = val else {
                return Err(assertion(val, "string"));
            };
            match to_id {
                TypeId::STRING => ok(Value::Str(vc.clone())),
                TypeId::BINARY => ok(Value::Bytes(vc.as_bytes().to_vec())),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        TypeId::VFLOAT => {
            let Value::VFloat(vc) = val else {
                return Err(assertion(val, "[]float32"));
            };
            match to_id {
                TypeId::BINARY => ok(Value::Bytes(float_array_as_bytes(vc))),
                TypeId::STRING => ok(Value::str(float_array_as_string(vc))),
                _ => Err(cant_convert(from_id, to_id)),
            }
        }
        _ => Err(cant_convert(from_id, to_id)),
    }
}

/// Dgraph's `toBinary`: a converted value in its binary form, as [`marshal`] writes it to [`TypeId::BINARY`].
///
/// # Errors
///
/// [`marshal`]'s errors; for a nil value, Go's panic text: Go's `ValueForType` of binary holds a `*[]byte`, on which
/// `toBinary`'s assertion panics.
pub fn marshal_binary(v: &Val) -> Result<Vec<u8>, TypesError> {
    let not_bytes = |go_type: &str| {
        TypesError::conversion(format!(
            "interface conversion: interface {{}} is {go_type}, not []uint8"
        ))
    };
    if v.value().is_none() {
        return Err(not_bytes("*[]uint8"));
    }
    match marshal(v, TypeId::BINARY)?.into_value() {
        Some(Value::Bytes(b)) => Ok(b),
        // marshal writes every value it accepts to binary as bytes.
        other => Err(not_bytes(other.as_ref().map_or("<nil>", Value::go_type))),
    }
}

fn to_binary(id: TypeId, value: &Value) -> Result<Vec<u8>, TypesError> {
    marshal_binary(&Val::new(id, value.clone()))
}

fn expected(go_type: &str, v: &Value) -> TypesError {
    TypesError::conversion(format!(
        "Expected value of type {go_type}. Got : {}",
        go_value(v)
    ))
}

/// Dgraph's `ObjectValue`: a converted value as the dgo API's `Value`. Geo, datetime, bigfloat and vector values
/// go in their binary forms. On error Go also returns an empty string value, which this port leaves out.
pub fn object_value(id: TypeId, value: &Value) -> Result<api::Value, TypesError> {
    use api::value::Val as V;
    let v = match (id, value) {
        (TypeId::STRING, Value::Str(s)) => V::StrVal(s.clone()),
        (TypeId::STRING, v) => return Err(expected("string", v)),
        (TypeId::DEFAULT, Value::Str(s)) => V::DefaultVal(s.clone()),
        (TypeId::DEFAULT, v) => return Err(expected("string", v)),
        (TypeId::INT, Value::Int(i)) => V::IntVal(*i),
        (TypeId::INT, v) => return Err(expected("int64", v)),
        (TypeId::FLOAT, Value::Float(f)) => V::DoubleVal(*f),
        (TypeId::FLOAT, v) => return Err(expected("float64", v)),
        (TypeId::BOOL, Value::Bool(b)) => V::BoolVal(*b),
        (TypeId::BOOL, v) => return Err(expected("bool", v)),
        (TypeId::BINARY, Value::Bytes(b)) => V::BytesVal(b.clone()),
        (TypeId::BINARY, v) => return Err(expected("[]byte", v)),
        (TypeId::BIGFLOAT, v) => V::BigfloatVal(to_binary(id, v)?),
        (TypeId::GEO, v) => V::GeoVal(to_binary(id, v)?),
        (TypeId::DATETIME, v) => V::DatetimeVal(to_binary(id, v)?),
        (TypeId::PASSWORD, Value::Str(s)) => V::PasswordVal(s.clone()),
        (TypeId::PASSWORD, v) => {
            return Err(TypesError::conversion(format!(
                "Expected value of type password. Got type: {}",
                v.go_type()
            )));
        }
        (TypeId::VFLOAT, v) => V::Vfloat32Val(to_binary(id, v)?),
        _ => {
            return Err(TypesError::conversion(format!(
                "ObjectValue not available for: {}",
                id.enum_value()
            )));
        }
    };
    Ok(api::Value { val: Some(v) })
}

/// Dgraph's `Val.MarshalJSON`: a converted value in Dgraph's JSON output. A bigfloat is its unquoted
/// `MarshalText`; a geometry is GeoJSON.
pub fn marshal_json(v: &Val) -> Result<Vec<u8>, TypesError> {
    let unsupported =
        |what: &str| TypesError::conversion(format!("json: unsupported value: {what}"));
    let text = match (v.tid(), v.value()) {
        (TypeId::INT, Some(Value::Int(i))) => i.to_string(),
        (TypeId::BOOL, Some(Value::Bool(b))) => go_strconv::format_bool(*b).into(),
        (TypeId::FLOAT, Some(Value::Float(f))) => {
            json_float(*f).ok_or_else(|| unsupported(&go_strconv::format_float(*f, b'g', 64)))?
        }
        (TypeId::DATETIME, Some(Value::DateTime(t))) => {
            // Go 1.27's json.Marshal reports the time's range error as a MarshalerError, without the
            // `Time.MarshalJSON: ` prefix its MarshalJSON adds.
            let s = t.marshal_text().map_err(|e| {
                TypesError::time(e.message().replacen(
                    "Time.MarshalText: ",
                    "json: error calling MarshalJSON for type *time.Time: ",
                    1,
                ))
            })?;
            format!("\"{s}\"")
        }
        (TypeId::GEO, Some(Value::Geo(g))) => g.to_geojson().ok_or_else(|| unsupported("NaN"))?,
        (TypeId::BIGFLOAT, Some(Value::BigFloat(b))) => b.marshal_text(),
        (TypeId::STRING | TypeId::DEFAULT, None) => crate::utils::json::encode_string(b""),
        (TypeId::STRING | TypeId::DEFAULT | TypeId::PASSWORD, Some(Value::Str(s))) => {
            crate::utils::json::encode_string(s.as_bytes())
        }
        (
            TypeId::INT
            | TypeId::BOOL
            | TypeId::FLOAT
            | TypeId::DATETIME
            | TypeId::GEO
            | TypeId::BIGFLOAT
            | TypeId::PASSWORD,
            Some(val),
        ) => return Err(assertion(val, "the type's Go value")),
        (t, _) => {
            return Err(TypesError::conversion(format!(
                "invalid type for MarshalJSON: {}",
                t.enum_value()
            )));
        }
    };
    Ok(text.into_bytes())
}
