/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/json_parser.go. Changed for ultragraphdb: translated to Rust; serde_json
 * in place of encoding/json and simdjson.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::{BTreeMap, HashMap};
use std::hash::{BuildHasher, Hasher};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use ultragraphdb_core_facets::{facet_for, to_binary};
use ultragraphdb_core_keys::STAR;
use ultragraphdb_core_types::{
    Geometry, TypeId, Value, float_array_as_bytes, object_value, parse_time, parse_vfloat,
};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::facet::ValType;
use ultragraphdb_proto_dgraph::api::value::Val as ApiVal;
use ultragraphdb_proto_pb::pb;

use crate::errors::chunker_error::ChunkerError;
use crate::types::json_op::JsonOp;
use crate::types::json_value::JsonValue;
use crate::types::nquad_buffer::NQuadBuffer;
use crate::utils::go_strconv::{atoi, parse_int, parse_json_float};

pub(crate) fn strip_spaces(s: &str) -> String {
    s.chars().filter(|r| !r.is_whitespace()).collect()
}

pub(crate) fn star() -> api::Value {
    api::Value {
        val: Some(ApiVal::DefaultVal(STAR.to_string())),
    }
}

fn value(val: ApiVal) -> api::Value {
    api::Value { val: Some(val) }
}

/// A Go string as Go's `strconv.Quote` writes it, as far as `strconv.Unquote` reads it back: the backslash, the
/// quote and a newline are the only runes Unquote refuses unescaped.
fn quote_for_unquote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for r in s.chars() {
        match r {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            r => out.push(r),
        }
    }
    out.push('"');
    out
}

/// Dgraph's `handleBasicFacetsType`: a JSON facet value as a facet: a datetime string as a datetime, any other
/// string as a string with its term tokens, a number with a `.` as a float, any other as an int, a bool.
fn handle_basic_facets_type(key: &str, facet_val: &JsonValue) -> Result<api::Facet, String> {
    let (json_value, value_type) = match facet_val {
        JsonValue::String(v) => match parse_time(v) {
            Ok(t) => (Value::DateTime(t), ValType::Datetime),
            // FacetFor already converts the value to binary.
            Err(_) => return facet_for(key, &quote_for_unquote(v)).map_err(|e| e.to_string()),
        },
        JsonValue::Number(n) if n.contains('.') => (
            Value::Float(parse_json_float(n).map_err(|e| e.to_string())?),
            ValType::Float,
        ),
        JsonValue::Number(n) => (
            Value::Int(parse_int(n).map_err(|e| e.to_string())?),
            ValType::Int,
        ),
        JsonValue::Bool(b) => (Value::Bool(*b), ValType::Bool),
        _ => return Err("facet value can only be string/number/bool".to_string()),
    };
    to_binary(key, &json_value, value_type).map_err(|e| e.to_string())
}

/// Dgraph's `parseMapFacets`: the facets of a scalar list predicate, given as maps from a value's index to the
/// facet, e.g. `{"nickname": ["alice", "bob"], "nickname|kind": {"0": "friends"}}`; one map per facet.
pub(crate) fn parse_map_facets(
    m: &BTreeMap<String, JsonValue>,
    prefix: &str,
) -> Result<Vec<HashMap<i64, api::Facet>>, ChunkerError> {
    // This happens at root.
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    let mut map_slice = Vec::new();
    for (fname, facet_val) in m {
        if *facet_val == JsonValue::Null || !fname.starts_with(prefix) {
            continue;
        }
        let JsonValue::Object(fm) = facet_val else {
            return Err(ChunkerError::json_mutation(format!(
                "facets format should be of type map for scalarlist predicates, found: {} for facet: {fname}",
                facet_val.go_v()
            )));
        };
        let mut idx_map = HashMap::with_capacity(fm.len());
        for (sidx, val) in fm {
            let key = &fname[prefix.len()..];
            let wrap = |e: String| {
                ChunkerError::json_mutation(format!("facet: {fname}, index: {sidx}: {e}"))
            };
            let facet = handle_basic_facets_type(key, val).map_err(wrap)?;
            let idx = atoi(sidx).map_err(|e| wrap(e.to_string()))?;
            idx_map.insert(idx, facet);
        }
        map_slice.push(idx_map);
    }
    Ok(map_slice)
}

/// Dgraph's `parseScalarFacets`: the facets of a predicate, `<prefix><key>` keys with string, number or bool values.
pub(crate) fn parse_scalar_facets(
    m: &BTreeMap<String, JsonValue>,
    prefix: &str,
) -> Result<Vec<api::Facet>, ChunkerError> {
    // This happens at root.
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    m.iter()
        .filter(|(fname, v)| **v != JsonValue::Null && fname.starts_with(prefix))
        .map(|(fname, v)| {
            handle_basic_facets_type(&fname[prefix.len()..], v)
                .map_err(|e| ChunkerError::json_mutation(format!("facet: {fname}: {e}")))
        })
        .collect()
}

/// Dgraph's `handleBasicType`: a string, number or bool as the N-Quad's object. A number with `.`, `E` or `e` is a
/// double, any other an int; a string is a vector when it parses as one, `uid(...)` or `val(...)` an object id,
/// and a string otherwise. Deleting with `""` or `false` deletes the predicate's every value.
pub(crate) fn handle_basic_type(
    k: &str,
    v: &JsonValue,
    op: JsonOp,
    nq: &mut api::NQuad,
) -> Result<(), ChunkerError> {
    match v {
        JsonValue::Number(n) if n.contains(['.', 'E', 'e']) => {
            let f = parse_json_float(n).map_err(|e| ChunkerError::json_mutation(e.to_string()))?;
            nq.object_value = Some(value(ApiVal::DoubleVal(f)));
        }
        JsonValue::Number(n) => {
            let i = parse_int(n).map_err(|e| ChunkerError::json_mutation(e.to_string()))?;
            nq.object_value = Some(value(ApiVal::IntVal(i)));
        }
        JsonValue::String(v) => {
            // Default value is considered as S P * deletion.
            if v.is_empty() && op == JsonOp::Delete {
                nq.object_value = Some(star());
                return Ok(());
            }
            if let Ok(vf) = parse_vfloat(v)
                && !vf.is_empty()
            {
                nq.object_value = Some(value(ApiVal::Vfloat32Val(float_array_as_bytes(&vf))));
                return Ok(());
            }
            // Handle the uid function in upsert block
            let s = strip_spaces(v);
            if s.starts_with("uid(") || s.starts_with("val(") {
                if !s.ends_with(')') {
                    return Err(ChunkerError::json_mutation(format!(
                        "while processing '{s}', brackets are not closed properly"
                    )));
                }
                nq.object_id = s;
                return Ok(());
            }
            // In RDF, we assume everything is default (types.DefaultID), but in JSON we assume string (StringID).
            // But this value will be checked against the schema so we don't overshadow a password value
            // (types.PasswordID) - Issue#2623
            nq.object_value = Some(value(ApiVal::StrVal(v.clone())));
        }
        JsonValue::Bool(b) => {
            if !b && op == JsonOp::Delete {
                nq.object_value = Some(star());
                return Ok(());
            }
            nq.object_value = Some(value(ApiVal::BoolVal(*b)));
        }
        _ => {
            return Err(ChunkerError::json_mutation(format!(
                "unexpected type for val for attr: {k} while converting to nquad"
            )));
        }
    }
    Ok(())
}

/// Dgraph's `handleGeoType`: whether an object of exactly `type` and `coordinates` is a GeoJSON geometry, set as
/// the N-Quad's object. An object GeoJSON refuses is not a geometry, and parses as a node.
pub(crate) fn handle_geo_type(
    val: &BTreeMap<String, JsonValue>,
    nq: &mut api::NQuad,
) -> Result<bool, ChunkerError> {
    if val.len() != 2 || !val.contains_key("type") || !val.contains_key("coordinates") {
        return Ok(false);
    }
    let b = JsonValue::Object(val.clone()).marshal();
    try_parse_as_geo(&b, nq)
}

/// Dgraph's `tryParseAsGeo`.
fn try_parse_as_geo(b: &str, nq: &mut api::NQuad) -> Result<bool, ChunkerError> {
    let Ok(Some(g)) = Geometry::from_geojson(b) else {
        return Ok(false);
    };
    let geo = object_value(TypeId::GEO, &Value::Geo(g)).map_err(|_| {
        ChunkerError::json_mutation(format!("couldn't convert value: {b} to geo type"))
    })?;
    nq.object_value = Some(geo);
    Ok(true)
}

/// The index of the blank node of a JSON object without `uid`; process-wide, from 1.
static NEXT_IDX: AtomicU64 = AtomicU64::new(0);

/// A random number drawn once per process, against collisions with the blank nodes a user names.
fn random_id() -> u32 {
    static RANDOM_ID: OnceLock<u32> = OnceLock::new();
    // RandomState is seeded from the operating system's randomness; it is not cryptographic, nor needs to be.
    *RANDOM_ID.get_or_init(|| {
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish() as u32
    })
}

/// Dgraph's `getNextBlank`: `_:dg.<random id>.<index>`.
pub(crate) fn get_next_blank() -> String {
    let id = NEXT_IDX.fetch_add(1, Ordering::SeqCst) + 1;
    format!("_:dg.{}.{id}", random_id())
}

/// Dgraph's `ParseJSON`: every N-Quad of a JSON object or array of objects, with the type hints of their
/// predicates. All N-Quads are held at once.
///
/// Go's convenience function takes simdjson's path where the CPU supports it, and encoding/json's otherwise; this
/// port parses as encoding/json does, the path Go takes on arm64.
///
/// # Errors
///
/// As [`NQuadBuffer::parse_json`].
pub fn parse_json(b: &[u8], op: JsonOp) -> Result<(Vec<api::NQuad>, pb::Metadata), ChunkerError> {
    let (mut buf, rx) = NQuadBuffer::new(-1);
    buf.parse_json(b, op)?;
    buf.flush();
    let nqs = rx.recv().unwrap_or_default();
    Ok((nqs, buf.metadata()))
}
