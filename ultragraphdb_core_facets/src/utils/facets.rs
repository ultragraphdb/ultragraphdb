/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/facets/utils.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::get_term_tokens;
use ultragraphdb_core_types::{TypeId, Val, Value, convert, marshal, parse_time};
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::facet::ValType;
use ultragraphdb_proto_pb::pb;

use crate::errors::facets_error::{FacetsError, FacetsErrorEnum};
use crate::utils::go_strconv::{parse_float, parse_int_base0, unquote};

/// Dgraph's `SortAndValidate`: sorts the facets by key and refuses a key named twice.
///
/// # Errors
///
/// [`FacetsErrorEnum::RepeatedKey`] with the smallest repeated key.
pub fn sort_and_validate(fs: &mut [api::Facet]) -> Result<(), FacetsError> {
    fs.sort_by(|a, b| a.key.cmp(&b.key));
    match fs.windows(2).find(|w| w[0].key == w[1].key) {
        Some(w) => Err(FacetsError::new(FacetsErrorEnum::RepeatedKey(
            w[1].key.clone(),
        ))),
        None => Ok(()),
    }
}

/// Dgraph's `CopyFacets`: copies of the facets `param` asks for, every one when it asks for all keys; each copy
/// takes the alias of its key unless all keys are asked for. Both the facets and the requested keys are sorted.
/// Nothing is copied without `param`.
pub fn copy_facets(fcs: &[api::Facet], param: Option<&pb::FacetParams>) -> Vec<api::Facet> {
    let Some(param) = param else {
        return Vec::new();
    };
    let mut fs = Vec::new();
    let (mut kidx, mut fidx) = (0, 0);
    while (param.all_keys || kidx < param.param.len()) && fidx < fcs.len() {
        let f = &fcs[fidx];
        if param.all_keys || param.param[kidx].key == f.key {
            fs.push(api::Facet {
                key: f.key.clone(),
                value: f.value.clone(),
                val_type: f.val_type,
                tokens: Vec::new(),
                alias: if param.all_keys {
                    String::new()
                } else {
                    param.param[kidx].alias.clone()
                },
            });
            kidx += 1;
            fidx += 1;
        } else if f.key > param.param[kidx].key {
            kidx += 1;
        } else {
            fidx += 1;
        }
    }
    fs
}

/// Dgraph's `valAndValType`: the value a facet's text stands for and its type. The first that fits wins: an
/// empty or double-quoted string, an int (`strconv.ParseInt(val, 0, 64)`, so base prefixes and underscores), a
/// float, `true` or `false`, a datetime.
fn val_and_val_type(val: &str) -> Result<(Value, ValType), FacetsError> {
    if val.is_empty() {
        return Ok((Value::str(""), ValType::String));
    }
    // strings should be in quotes.
    if val.len() >= 2 && val.starts_with('"') && val.ends_with('"') {
        return match unquote(val) {
            Some(uq) => Ok((Value::Str(uq), ValType::String)),
            None => Err(FacetsError::new(FacetsErrorEnum::Unquote(val.to_string()))),
        };
    }
    match parse_int_base0(val) {
        Ok(i) => return Ok((Value::Int(i), ValType::Int)),
        // Only digits is a big integer; anything else may still be a float.
        Err(e) if e.range && val.bytes().all(|b| b.is_ascii_digit()) => {
            return Err(FacetsError::new(FacetsErrorEnum::Number(e.to_string())));
        }
        Err(_) => {}
    }
    match parse_float(val) {
        // NaN cannot be stored as it is: it serializes into invalid JSON.
        Ok(f) if f.is_nan() => return Err(FacetsError::new(FacetsErrorEnum::NaN)),
        Ok(f) => return Ok((Value::Float(f), ValType::Float)),
        Err(e) if e.range => return Err(FacetsError::new(FacetsErrorEnum::Number(e.to_string()))),
        Err(_) => {}
    }
    if val == "true" || val == "false" {
        return Ok((Value::Bool(val == "true"), ValType::Bool));
    }
    if let Ok(t) = parse_time(val) {
        return Ok((Value::DateTime(t), ValType::Datetime));
    }
    Err(FacetsError::new(FacetsErrorEnum::Unparsable(
        val.to_string(),
    )))
}

/// Dgraph's `FacetFor`: the facet `key` with the value `val` stands for, in its binary form; a string value also
/// gets its term tokens, sorted.
///
/// A `\x` or octal escape in a quoted value can write bytes that are not UTF-8. The value keeps them; the term
/// tokenizer splits words at them, as Go's does.
///
/// # Errors
///
/// [`FacetsErrorEnum::Unquote`] for a quoted value that is not a Go string literal, [`FacetsErrorEnum::Number`]
/// for an int or float out of range, [`FacetsErrorEnum::NaN`], and [`FacetsErrorEnum::Unparsable`] for a value of
/// no type, e.g. unquoted text.
pub fn facet_for(key: &str, val: &str) -> Result<api::Facet, FacetsError> {
    let (v, vt) = val_and_val_type(val)?;
    let mut facet = to_binary(key, &v, vt)?;
    if let Value::Str(s) = &v {
        let mut tokens = get_term_tokens(&[s])?;
        tokens.sort();
        facet.tokens = tokens
            .iter()
            .map(|t| String::from_utf8_lossy(t).into_owned())
            .collect();
    }
    Ok(facet)
}

/// Dgraph's `ToBinary`: the facet `key` with `value`, a value of `source_type`, in its binary form, without
/// tokens.
///
/// # Errors
///
/// [`FacetsErrorEnum::Types`] when `value` is not of `source_type`, where Go's type assertion panics.
pub fn to_binary(
    key: &str,
    value: &Value,
    source_type: ValType,
) -> Result<api::Facet, FacetsError> {
    let source_tid = type_id_for_val_type(source_type);
    let bytes = match marshal(&Val::new(source_tid, value.clone()), TypeId::BINARY)?.into_value() {
        Some(Value::Bytes(b)) => b,
        _ => Vec::new(),
    };
    Ok(api::Facet {
        key: key.to_string(),
        value: bytes,
        val_type: source_type as i32,
        tokens: Vec::new(),
        alias: String::new(),
    })
}

fn type_id_for_val_type(vt: ValType) -> TypeId {
    match vt {
        ValType::Int => TypeId::INT,
        ValType::Float => TypeId::FLOAT,
        ValType::Bool => TypeId::BOOL,
        ValType::Datetime => TypeId::DATETIME,
        ValType::String => TypeId::STRING,
    }
}

/// Dgraph's `TypeIDFor`: the scalar type of a facet's value.
///
/// # Errors
///
/// [`FacetsErrorEnum::UnrecognizedType`] when the facet's value type is not one the dgo API names.
pub fn type_id_for(f: &api::Facet) -> Result<TypeId, FacetsError> {
    ValType::try_from(f.val_type)
        .map(type_id_for_val_type)
        .map_err(|_| FacetsError::new(FacetsErrorEnum::UnrecognizedType(f.val_type)))
}

/// Dgraph's `ValFor`: a facet's value decoded from its binary form into its type.
///
/// # Errors
///
/// [`FacetsErrorEnum::UnrecognizedType`] for an unknown value type, [`FacetsErrorEnum::Types`] when the bytes do
/// not decode.
pub fn val_for(f: &api::Facet) -> Result<Val, FacetsError> {
    let facet_tid = type_id_for(f)?;
    Ok(convert(
        &Val::bytes(TypeId::BINARY, f.value.clone()),
        facet_tid,
    )?)
}
