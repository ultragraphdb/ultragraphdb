/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::Length;

/// Dgraph's `Length.String` (types/earth.go), as Go 1.27 prints each value.
#[test]
fn test_length_display() {
    let cases: [(f64, &str); 17] = [
        (0.0, "0.000 cm"),
        (-0.0, "-0.000 cm"),
        (-1.0, "-100.000 cm"),
        (0.99999, "99.999 cm"),
        (0.0125, "1.250 cm"),
        (0.5, "50.000 cm"),
        (1.0, "1.000 m"),
        (2.5, "2.500 m"),
        (12.0, "12.000 m"),
        (1000.0, "1000.000 m"),
        (1000.0005, "1.000 km"),
        (1500.0, "1.500 km"),
        (1e6, "1000.000 km"),
        (f64::INFINITY, "+Inf km"),
        (f64::NEG_INFINITY, "-Inf cm"),
        (f64::NAN, "NaN m"),
        (
            1e300,
            "1000000000000000017652801462756379714374878780719864776839443139119744823869255243069012222883470359078822072829219411228534934402712624705615450492327979456500795456339201761949451160807447294527656222743617592048849967890105831362861792425329827928397252374398383022243308510390698430058459037696.000 km",
        ),
    ];
    for (metres, want) in cases {
        assert_eq!(Length::new(metres).to_string(), want, "{metres}");
    }
}

#[test]
fn test_length_keeps_its_metres() {
    for m in [0.0, -2.5, 1e-300, f64::MAX, f64::INFINITY] {
        assert_eq!(Length::new(m).metres(), m);
    }
    assert!(Length::new(f64::NAN).metres().is_nan());
    assert!(Length::new(1.0) < Length::new(1.5));
}
