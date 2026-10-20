/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/latlng.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::gomath;
use crate::s2::point::Point;
use crate::s2::s1::Angle;
use core::f64::consts::FRAC_PI_2;
use core::f64::consts::PI;

/// A point on the unit sphere as a pair of angles.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LatLng {
    pub(crate) lat: Angle,
    pub(crate) lng: Angle,
}

impl LatLng {
    /// Returns the LatLng with the given latitude and longitude.
    pub const fn new(lat: Angle, lng: Angle) -> LatLng {
        LatLng { lat, lng }
    }

    /// Returns a LatLng for the coordinates given in degrees.
    pub fn from_degrees(lat: f64, lng: f64) -> LatLng {
        LatLng {
            lat: Angle::from_degrees(lat),
            lng: Angle::from_degrees(lng),
        }
    }

    /// Returns a LatLng for a given Point.
    pub fn from_point(p: Point) -> LatLng {
        LatLng {
            lat: latitude(p),
            lng: longitude(p),
        }
    }

    /// The latitude.
    pub fn lat(&self) -> Angle {
        self.lat
    }

    /// The longitude.
    pub fn lng(&self) -> Angle {
        self.lng
    }

    /// Returns true iff the LatLng is normalized, with lat ∈ [-π/2,π/2] and lng ∈ [-π,π].
    pub fn is_valid(&self) -> bool {
        self.lat.radians().abs() <= FRAC_PI_2 && self.lng.radians().abs() <= PI
    }
}

/// The latitude of a point, as golang/geo's `latitude`.
pub(crate) fn latitude(p: Point) -> Angle {
    let v = p.0;
    Angle(gomath::atan2(v.z, (v.x * v.x + v.y * v.y).sqrt()))
}

/// The longitude of a point, as golang/geo's `longitude`.
pub(crate) fn longitude(p: Point) -> Angle {
    Angle(gomath::atan2(p.0.y, p.0.x))
}
