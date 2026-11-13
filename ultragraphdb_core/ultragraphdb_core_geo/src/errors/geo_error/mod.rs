/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod geo_error_display;
mod geo_error_error;

/// Why a geometry was refused.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GeoError(pub GeoErrorEnum);

/// The reasons a geometry is refused, each with its message.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GeoErrorEnum {
    /// The text is no JSON document.
    Json(String),
    /// The JSON document is no GeoJSON geometry: a member is missing or of the wrong kind.
    Structure(String),
    /// The geometry's type is none this crate reads.
    UnsupportedType(String),
    /// A coordinate is not finite or lies outside CRS84's range.
    Coordinate(String),
    /// A geometry's shape breaks a rule: a line too short, a ring open, positions of mixed dimension.
    Shape(String),
    /// The bytes are no well-known binary geometry.
    Wkb(String),
}

impl GeoError {
    /// A [`GeoErrorEnum::Json`] error with its message.
    pub fn json(message: impl Into<String>) -> Self {
        Self(GeoErrorEnum::Json(message.into()))
    }

    /// A [`GeoErrorEnum::Structure`] error with its message.
    pub fn structure(message: impl Into<String>) -> Self {
        Self(GeoErrorEnum::Structure(message.into()))
    }

    /// A [`GeoErrorEnum::UnsupportedType`] error naming the type.
    pub fn unsupported_type(name: impl Into<String>) -> Self {
        Self(GeoErrorEnum::UnsupportedType(name.into()))
    }

    /// A [`GeoErrorEnum::Coordinate`] error with its message.
    pub fn coordinate(message: impl Into<String>) -> Self {
        Self(GeoErrorEnum::Coordinate(message.into()))
    }

    /// A [`GeoErrorEnum::Shape`] error with its message.
    pub fn shape(message: impl Into<String>) -> Self {
        Self(GeoErrorEnum::Shape(message.into()))
    }

    /// A [`GeoErrorEnum::Wkb`] error with its message.
    pub fn wkb(message: impl Into<String>) -> Self {
        Self(GeoErrorEnum::Wkb(message.into()))
    }
}
