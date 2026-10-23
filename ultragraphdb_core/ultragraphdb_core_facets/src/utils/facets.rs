/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/facets/utils.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_core_tok::get_term_tokens;
use ultragraphdb_core_types::{TypeId, Val, convert, parse_time};
use ultragraphdb_error_num::NumErrorEnum;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::facet::ValType;
use ultragraphdb_proto_pb::pb;

use crate::errors::facets_error::{FacetsError, FacetsErrorEnum};
use crate::types::facet_value::FacetValue;

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
/// float, `true` or `false`, a datetime. A quoted value whose escapes write bytes that are not UTF-8 is refused
/// for `key`.
fn facet_value(key: &str, val: &str) -> Result<FacetValue, FacetsError> {
    if val.is_empty() {
        return Ok(FacetValue::Str(String::new()));
    }
    // strings should be in quotes.
    if val.len() >= 2 && val.starts_with('"') && val.ends_with('"') {
        let Some(uq) = go_strconv::unquote(val) else {
            return Err(FacetsError::new(FacetsErrorEnum::Unquote(val.to_string())));
        };
        return String::from_utf8(uq)
            .map(FacetValue::Str)
            .map_err(|_| FacetsError::new(FacetsErrorEnum::InvalidUtf8(key.to_string())));
    }
    match go_strconv::parse_int(val, 0) {
        Ok(i) => return Ok(FacetValue::Int(i)),
        // Only digits is a big integer; anything else may still be a float.
        Err(e)
            if matches!(e.0, NumErrorEnum::Range { .. })
                && val.bytes().all(|b| b.is_ascii_digit()) =>
        {
            return Err(FacetsError::new(FacetsErrorEnum::Number(e.to_string())));
        }
        Err(_) => {}
    }
    match go_strconv::parse_float(val, 64) {
        // NaN cannot be stored as it is: it serializes into invalid JSON.
        Ok(f) if f.is_nan() => return Err(FacetsError::new(FacetsErrorEnum::NaN)),
        Ok(f) => return Ok(FacetValue::Float(f)),
        Err(e) if matches!(e.0, NumErrorEnum::Range { .. }) => {
            return Err(FacetsError::new(FacetsErrorEnum::Number(e.to_string())));
        }
        Err(_) => {}
    }
    if val == "true" || val == "false" {
        return Ok(FacetValue::Bool(val == "true"));
    }
    if let Ok(t) = parse_time(val) {
        return Ok(FacetValue::DateTime(t));
    }
    Err(FacetsError::new(FacetsErrorEnum::Unparsable(
        val.to_string(),
    )))
}

/// Dgraph's `FacetFor`: the facet `key` with the value `val` stands for, in its binary form; a string value also
/// gets its term tokens, sorted.
///
/// A `\x` or octal escape in a quoted value can write bytes that are not UTF-8. Dgraph keeps them and tokenizes
/// them; this port refuses the facet, as a string value is valid UTF-8 (ledger 17, 18, 31).
///
/// # Errors
///
/// [`FacetsErrorEnum::Unquote`] for a quoted value that is not a Go string literal, [`FacetsErrorEnum::InvalidUtf8`]
/// for one whose escapes write bytes that are not UTF-8, [`FacetsErrorEnum::Number`] for an int or float out of
/// range, [`FacetsErrorEnum::NaN`], and [`FacetsErrorEnum::Unparsable`] for a value of no type, e.g. unquoted text.
pub fn facet_for(key: &str, val: &str) -> Result<api::Facet, FacetsError> {
    let v = facet_value(key, val)?;
    let mut facet = v.to_binary(key)?;
    if let FacetValue::Str(s) = &v {
        let mut tokens = get_term_tokens(&[s])?;
        tokens.sort();
        facet.tokens = tokens
            .iter()
            .map(|t| String::from_utf8_lossy(t).into_owned())
            .collect();
    }
    Ok(facet)
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
