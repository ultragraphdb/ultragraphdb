/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::{EARTH_RADIUS_METERS, earth_angle, earth_area, earth_distance};

#[test]
fn test_earth_conversions() {
    assert_eq!(earth_angle(EARTH_RADIUS_METERS).radians(), 1.0);
    assert!((earth_distance(earth_angle(1000.0)).metres() - 1000.0).abs() < 1e-9);
    assert_eq!(
        earth_area(1.0).square_metres(),
        EARTH_RADIUS_METERS * EARTH_RADIUS_METERS
    );
    assert_eq!(earth_distance(earth_angle(1500.0)).to_string(), "1.500 km");
}

#[test]
fn test_earth_conversions_at_zero_and_below() {
    // Dgraph's EarthRadiusMeters is 1000 * 6371; the conversions are plain products, sign included.
    assert_eq!(EARTH_RADIUS_METERS, 6_371_000.0);
    assert_eq!(earth_angle(0.0).radians(), 0.0);
    assert_eq!(earth_angle(-6_371_000.0).radians(), -1.0);
    assert_eq!(
        earth_distance(earth_angle(-6_371_000.0)).metres(),
        -6_371_000.0
    );
    assert_eq!(earth_area(0.0).square_metres(), 0.0);
    assert_eq!(
        earth_area(0.5).square_metres(),
        0.5 * 6_371_000.0 * 6_371_000.0
    );
    assert!(earth_angle(f64::NAN).radians().is_nan());
}
