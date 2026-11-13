/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::{GeoError, GeoErrorEnum};

impl fmt::Display for GeoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            GeoErrorEnum::Json(message) => write!(f, "geojson: {message}"),
            GeoErrorEnum::Structure(message) => write!(f, "geojson: {message}"),
            GeoErrorEnum::UnsupportedType(name) => write!(f, "geojson: unsupported type: {name}"),
            GeoErrorEnum::Coordinate(message) => write!(f, "geometry: {message}"),
            GeoErrorEnum::Shape(message) => write!(f, "geometry: {message}"),
            GeoErrorEnum::Wkb(message) => write!(f, "wkb: {message}"),
        }
    }
}
