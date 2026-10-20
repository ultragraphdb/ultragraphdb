/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/earth.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::Angle;
use crate::types::area::Area;
use crate::types::length::Length;

/// Dgraph's `EarthRadiusMeters`: a spherical Earth's radius.
pub const EARTH_RADIUS_METERS: f64 = 1000.0 * 6371.0;

/// Dgraph's `EarthDistance`: an angle on the unit sphere as a distance on Earth.
pub fn earth_distance(angle: Angle) -> Length {
    Length::new(angle.radians() * EARTH_RADIUS_METERS)
}

/// Dgraph's `EarthAngle`: a distance on Earth in metres as an angle on the unit sphere.
pub fn earth_angle(dist: f64) -> Angle {
    Angle::from_radians(dist / EARTH_RADIUS_METERS)
}

/// Dgraph's `EarthArea`: an area on the unit sphere as an area on Earth.
pub fn earth_area(a: f64) -> Area {
    Area::new(a * EARTH_RADIUS_METERS * EARTH_RADIUS_METERS)
}
