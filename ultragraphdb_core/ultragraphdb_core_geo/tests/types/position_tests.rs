/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_geo::{GeoErrorEnum, Position};
use ultragraphdb_tool_bdd::TestResult;

/// A position within CRS84's range is kept as given, its bounds included.
#[test]
fn test_a_position_in_range_is_kept() -> TestResult {
    for (lng, lat) in [(8.63, 50.12), (-180.0, -90.0), (180.0, 90.0), (0.0, 0.0)] {
        let position = Position::new(lng, lat)?;
        assert_eq!((position.lng(), position.lat()), (lng, lat));
        assert_eq!(position.elevation(), None);
    }
    Ok(())
}

/// A coordinate that is not finite, or lies past the range, is refused as a coordinate.
#[test]
fn test_a_position_out_of_range_is_refused() {
    for (lng, lat) in [
        (f64::NAN, 0.0),
        (0.0, f64::NAN),
        (f64::INFINITY, 0.0),
        (0.0, f64::NEG_INFINITY),
        (180.000_001, 0.0),
        (-180.000_001, 0.0),
        (0.0, 90.000_001),
        (0.0, -90.000_001),
    ] {
        assert!(
            matches!(Position::new(lng, lat), Err(e) if matches!(e.0, GeoErrorEnum::Coordinate(_))),
            "{lng}, {lat}"
        );
    }
}

/// An elevation is kept beside the position it was added to; one that is not finite is refused.
#[test]
fn test_an_elevation_is_kept_and_a_non_finite_one_refused() -> TestResult {
    let position = Position::new(8.63, 50.12)?;
    let raised = position.with_elevation(-12.5)?;
    assert_eq!(raised.elevation(), Some(-12.5));
    assert_eq!(
        (raised.lng(), raised.lat()),
        (position.lng(), position.lat())
    );
    for metres in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            position.with_elevation(metres),
            Err(e) if matches!(e.0, GeoErrorEnum::Coordinate(_))
        ));
    }
    Ok(())
}
