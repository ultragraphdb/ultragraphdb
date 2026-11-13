/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::geo_error::GeoError;

/// A position in CRS84: a longitude and a latitude in degrees, and an elevation in metres where one is stated. Every
/// coordinate is finite, the latitude from -90 to 90 and the longitude from -180 to 180.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    lng: f64,
    lat: f64,
    elevation: Option<f64>,
}

impl Position {
    /// The position at `lng` and `lat` degrees.
    ///
    /// # Errors
    ///
    /// [`GeoError::coordinate`] for a coordinate that is NaN or infinite, a latitude outside -90 to 90 or a longitude
    /// outside -180 to 180.
    pub fn new(lng: f64, lat: f64) -> Result<Self, GeoError> {
        if !(lng.is_finite() && lat.is_finite()) {
            return Err(GeoError::coordinate(format!(
                "a position of {lng}, {lat} is not finite"
            )));
        }
        if !(-90.0..=90.0).contains(&lat) {
            return Err(GeoError::coordinate(format!(
                "a latitude of {lat} lies outside -90 to 90"
            )));
        }
        if !(-180.0..=180.0).contains(&lng) {
            return Err(GeoError::coordinate(format!(
                "a longitude of {lng} lies outside -180 to 180"
            )));
        }
        Ok(Self {
            lng,
            lat,
            elevation: None,
        })
    }

    /// The position at `metres` of elevation.
    ///
    /// # Errors
    ///
    /// [`GeoError::coordinate`] for an elevation that is NaN or infinite.
    pub fn with_elevation(self, metres: f64) -> Result<Self, GeoError> {
        if !metres.is_finite() {
            return Err(GeoError::coordinate(format!(
                "an elevation of {metres} is not finite"
            )));
        }
        Ok(Self {
            elevation: Some(metres),
            ..self
        })
    }

    /// The longitude, in degrees.
    #[must_use]
    pub fn lng(&self) -> f64 {
        self.lng
    }

    /// The latitude, in degrees.
    #[must_use]
    pub fn lat(&self) -> f64 {
        self.lat
    }

    /// The elevation, in metres, where one is stated.
    #[must_use]
    pub fn elevation(&self) -> Option<f64> {
        self.elevation
    }
}
