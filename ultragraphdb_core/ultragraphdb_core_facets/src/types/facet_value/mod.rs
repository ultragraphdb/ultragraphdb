/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/facets/utils.go (ToBinary) and types/conversion.go (Marshal). Changed for
 * ultragraphdb: the value and its facet type are one enum.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_types::DateTime;
use ultragraphdb_proto_dgraph::api;
use ultragraphdb_proto_dgraph::api::facet::ValType;

use crate::errors::facets_error::FacetsError;

/// A facet value with its type: Dgraph's `(interface{}, api.Facet_ValType)` pair, whose Go value can be of another
/// type than the facet type names, on which `ToBinary` panics (ledger 20). A string stays bytes, because a `\x`
/// escape in a facet can write bytes that are not UTF-8.
#[derive(Debug, Clone, PartialEq)]
pub enum FacetValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    DateTime(DateTime),
    Str(Vec<u8>),
}

impl FacetValue {
    /// The facet type of the value.
    pub fn val_type(&self) -> ValType {
        match self {
            FacetValue::Int(_) => ValType::Int,
            FacetValue::Float(_) => ValType::Float,
            FacetValue::Bool(_) => ValType::Bool,
            FacetValue::DateTime(_) => ValType::Datetime,
            FacetValue::Str(_) => ValType::String,
        }
    }

    /// Dgraph's `ToBinary`: the facet `key` with this value in its binary form, as `Marshal` writes it, without
    /// tokens.
    ///
    /// # Errors
    ///
    /// [`FacetsErrorEnum::Types`](crate::FacetsErrorEnum::Types) when a datetime's zone offset cannot be encoded.
    pub fn to_binary(&self, key: &str) -> Result<api::Facet, FacetsError> {
        let value = match self {
            FacetValue::Int(i) => i.to_le_bytes().to_vec(),
            FacetValue::Float(f) => f.to_bits().to_le_bytes().to_vec(),
            FacetValue::Bool(b) => vec![u8::from(*b)],
            FacetValue::DateTime(t) => t.marshal_binary()?,
            FacetValue::Str(s) => s.clone(),
        };
        Ok(api::Facet {
            key: key.to_string(),
            value,
            val_type: self.val_type() as i32,
            tokens: Vec::new(),
            alias: String::new(),
        })
    }
}
