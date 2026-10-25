/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::Area;

/// Dgraph's `Area.String` (types/earth.go), as Go 1.26 prints each value.
#[test]
fn test_area_display() {
    let cases: [(f64, &str); 18] = [
        (0.0, "0.000 cm^2"),
        (-0.0, "-0.000 cm^2"),
        (-1.0, "-10000.000 cm^2"),
        (0.99999, "9999.900 cm^2"),
        (0.0004999, "4.999 cm^2"),
        (0.00001, "0.100 cm^2"),
        (0.5, "5000.000 cm^2"),
        (1.0, "1.000 m^2"),
        (12.0, "12.000 m^2"),
        (1000.0005, "1000.000 m^2"),
        (1e6, "1000000.000 m^2"),
        (1000000.5, "1.000 km^2"),
        (2.5e6, "2.500 km^2"),
        (f64::INFINITY, "+Inf km^2"),
        (f64::NEG_INFINITY, "-Inf cm^2"),
        (f64::NAN, "NaN m^2"),
        (
            1e300,
            "1000000000000000066436467741248103118547156170586292454485461107376856746627884050583544890346687569804406120783567460668037744292161050890877875387371120199760770880078039125129799472606133954939884328574613293205683935969567348590731356020719265634967118123751637393518591968740451429495341056.000 km^2",
        ),
        (2.5, "2.500 m^2"),
    ];
    for (square_metres, want) in cases {
        assert_eq!(
            Area::new(square_metres).to_string(),
            want,
            "{square_metres}"
        );
    }
}

#[test]
fn test_area_keeps_its_square_metres() {
    for a in [0.0, -2.5, 1e-300, f64::MAX, f64::INFINITY] {
        assert_eq!(Area::new(a).square_metres(), a);
    }
    assert!(Area::new(f64::NAN).square_metres().is_nan());
    assert!(Area::new(1.0) < Area::new(1.5));
}
