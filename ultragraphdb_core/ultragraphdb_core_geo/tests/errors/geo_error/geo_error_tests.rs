/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_core_geo::{GeoError, GeoErrorEnum};

/// Each constructor builds its variant with the message it was given.
#[test]
fn test_each_constructor_builds_its_variant() {
    assert_eq!(
        GeoError::json("m"),
        GeoError(GeoErrorEnum::Json("m".to_owned()))
    );
    assert_eq!(
        GeoError::structure("m"),
        GeoError(GeoErrorEnum::Structure("m".to_owned()))
    );
    assert_eq!(
        GeoError::unsupported_type("m"),
        GeoError(GeoErrorEnum::UnsupportedType("m".to_owned()))
    );
    assert_eq!(
        GeoError::coordinate("m"),
        GeoError(GeoErrorEnum::Coordinate("m".to_owned()))
    );
    assert_eq!(
        GeoError::shape("m"),
        GeoError(GeoErrorEnum::Shape("m".to_owned()))
    );
    assert_eq!(
        GeoError::wkb("m"),
        GeoError(GeoErrorEnum::Wkb("m".to_owned()))
    );
}

/// Each variant's text names the form that refused, then the message.
#[test]
fn test_each_variant_displays_its_form_and_message() {
    for (error, expected) in [
        (GeoError::json("m"), "geojson: m"),
        (GeoError::structure("m"), "geojson: m"),
        (
            GeoError::unsupported_type("Circle"),
            "geojson: unsupported type: Circle",
        ),
        (GeoError::coordinate("m"), "geometry: m"),
        (GeoError::shape("m"), "geometry: m"),
        (GeoError::wkb("m"), "wkb: m"),
    ] {
        assert_eq!(error.to_string(), expected);
    }
}

/// The error is a standard error with no source beneath it.
#[test]
fn test_the_error_is_a_standard_error() {
    assert!(GeoError::wkb("m").source().is_none());
}
