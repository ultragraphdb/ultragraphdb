/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/facets/utils.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod facets_error_display;
mod facets_error_error;

use ultragraphdb_core_tok::TokError;
use ultragraphdb_core_types::TypesError;

/// Why a facet could not be parsed, encoded, decoded or validated. `Display` gives Dgraph's error text, which
/// clients match on.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FacetsError(pub FacetsErrorEnum);

/// The classification of a [`FacetsError`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FacetsErrorEnum {
    /// A facet list names this key twice: `Repeated keys are not allowed in facets. But got <key>`.
    RepeatedKey(String),
    /// This quoted value is not a Go string literal: `could not unquote "<value>":: invalid syntax`.
    Unquote(String),
    /// The quoted value of the facet with this key writes bytes that are not UTF-8:
    /// `Facet value of <key> is not valid UTF-8`.
    InvalidUtf8(String),
    /// A number does not fit its type; the message is Go's `strconv` error, e.g.
    /// `strconv.ParseInt: parsing "99999999999999999999": value out of range`.
    Number(String),
    /// The value is a float that is not a number: `Got invalid value: NaN`.
    NaN,
    /// The value is none of string, int, float, bool or datetime: `Could not parse the facet value : [<value>]`.
    Unparsable(String),
    /// A facet's value type is not one the dgo API names: `Unrecognized facet type: <n>`.
    UnrecognizedType(i32),
    /// The value could not be encoded or decoded.
    Types(TypesError),
    /// The term tokens of a string value could not be built.
    Tok(TokError),
}

impl FacetsError {
    pub(crate) fn new(kind: FacetsErrorEnum) -> Self {
        Self(kind)
    }

    #[allow(non_snake_case)]
    pub fn RepeatedKey(msg: String) -> Self {
        Self(FacetsErrorEnum::RepeatedKey(msg))
    }

    #[allow(non_snake_case)]
    pub fn Unquote(msg: String) -> Self {
        Self(FacetsErrorEnum::Unquote(msg))
    }

    #[allow(non_snake_case)]
    pub fn InvalidUtf8(key: String) -> Self {
        Self(FacetsErrorEnum::InvalidUtf8(key))
    }

    #[allow(non_snake_case)]
    pub fn Number(msg: String) -> Self {
        Self(FacetsErrorEnum::Number(msg))
    }

    #[allow(non_snake_case)]
    pub fn NaN() -> Self {
        Self(FacetsErrorEnum::NaN)
    }

    #[allow(non_snake_case)]
    pub fn Unparsable(msg: String) -> Self {
        Self(FacetsErrorEnum::Unparsable(msg))
    }

    #[allow(non_snake_case)]
    pub fn UnrecognizedType(value: i32) -> Self {
        Self(FacetsErrorEnum::UnrecognizedType(value))
    }

    #[allow(non_snake_case)]
    pub fn Types(value: TypesError) -> Self {
        Self(FacetsErrorEnum::Types(value))
    }

    #[allow(non_snake_case)]
    pub fn Tok(value: TokError) -> Self {
        Self(FacetsErrorEnum::Tok(value))
    }
}

impl From<TypesError> for FacetsError {
    fn from(e: TypesError) -> Self {
        Self::new(FacetsErrorEnum::Types(e))
    }
}

impl From<TokError> for FacetsError {
    fn from(e: TokError) -> Self {
        Self::new(FacetsErrorEnum::Tok(e))
    }
}
