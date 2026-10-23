/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/latlng_test.go; the golden-vector checks are new.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! LatLng ↔ Point conversions and the leaf cell of a LatLng, against Go.

use super::golden::{Checker, cell, f, hx, load, same, same_point};
use serde::Deserialize;
use ultragraphdb_core_types::{CellId, LatLng, Point};

#[derive(Deserialize)]
struct Case {
    lat_deg: String,
    lng_deg: String,
    lat_rad: String,
    lng_rad: String,
    point: [String; 3],
    back_lat: String,
    back_lng: String,
    back_lat_deg: String,
    back_lng_deg: String,
    cell: String,
    face: usize,
    level: i64,
    tokens: Vec<String>,
    levels: Vec<i64>,
}

#[test]
fn latlng_point_and_cell_match_go() {
    let cases: Vec<Case> = load("latlng.json");
    let mut c = Checker::new("latlng");
    for k in &cases {
        let ll = LatLng::from_degrees(f(&k.lat_deg), f(&k.lng_deg));
        let tag = format!("({}, {})", k.lat_deg, k.lng_deg);
        c.check(
            same(ll.lat().radians(), f(&k.lat_rad)) && same(ll.lng().radians(), f(&k.lng_rad)),
            || {
                format!(
                    "{tag} radians {} {} want {} {}",
                    hx(ll.lat().radians()),
                    hx(ll.lng().radians()),
                    k.lat_rad,
                    k.lng_rad
                )
            },
        );
        let p = Point::from_lat_lng(ll);
        c.check(same_point(p, &k.point), || {
            format!("{tag} point {p:?} want {:?}", k.point)
        });
        let back = LatLng::from_point(p);
        c.check(
            same(back.lat().radians(), f(&k.back_lat))
                && same(back.lng().radians(), f(&k.back_lng)),
            || {
                format!(
                    "{tag} back {} {} want {} {}",
                    hx(back.lat().radians()),
                    hx(back.lng().radians()),
                    k.back_lat,
                    k.back_lng
                )
            },
        );
        c.check(
            same(back.lat().degrees(), f(&k.back_lat_deg))
                && same(back.lng().degrees(), f(&k.back_lng_deg)),
            || {
                format!(
                    "{tag} back degrees {} {}",
                    hx(back.lat().degrees()),
                    hx(back.lng().degrees())
                )
            },
        );
        let id = CellId::from_lat_lng(ll);
        c.check(id == cell(&k.cell), || {
            format!("{tag} cell {:016x} want {}", id.id(), k.cell)
        });
        c.check(id.face() == k.face && id.level() == k.level, || {
            format!("{tag} face/level {} {}", id.face(), id.level())
        });
        for (l, (t, lv)) in k.tokens.iter().zip(&k.levels).enumerate() {
            let parent = id.parent(l as i64);
            c.check(&parent.to_token() == t && parent.level() == *lv, || {
                format!(
                    "{tag} parent({l}) {} level {} want {t} {lv}",
                    parent.to_token(),
                    parent.level()
                )
            });
        }
    }
    c.finish();
}

// Ported from golang/geo s2/latlng_test.go, TestLatLngPointConversion.
#[test]
fn latlng_point_conversion() {
    // lat, lng, x, y, z
    let tests: [(f64, f64, f64, f64, f64); 6] = [
        (0.0, 0.0, 1.0, 0.0, 0.0),
        (90.0, 0.0, 6.12e-17, 0.0, 1.0),
        (-90.0, 0.0, 6.12e-17, 0.0, -1.0),
        (0.0, 180.0, -1.0, 1.22e-16, 0.0),
        (0.0, -180.0, -1.0, -1.22e-16, 0.0),
        (90.0, 180.0, -6.12e-17, 7.49e-33, 1.0),
    ];
    for (lat, lng, x, y, z) in tests {
        let p = Point::from_lat_lng(LatLng::from_degrees(lat, lng));
        assert!((p.x() - x).abs() <= 1e-16, "({lat}, {lng}) x {}", p.x());
        assert!((p.y() - y).abs() <= 1e-16, "({lat}, {lng}) y {}", p.y());
        assert!((p.z() - z).abs() <= 1e-16, "({lat}, {lng}) z {}", p.z());
        let ll = LatLng::from_point(p);
        // We need to be careful here, since if the latitude is +/- 90, any longitude
        // is now a valid conversion.
        let is_polar = lat == 90.0 || lat == -90.0;
        assert!(
            (ll.lat().degrees() - lat).abs() <= 1e-13,
            "({lat}, {lng}) lat {}",
            ll.lat().degrees()
        );
        if !is_polar {
            assert!(
                (ll.lng().degrees() - lng).abs() <= 1e-13,
                "({lat}, {lng}) lng {}",
                ll.lng().degrees()
            );
        }
    }
}

// Ported from golang/geo s2/latlng_test.go (IsValid cases of TestLatLngNormalized).
#[test]
fn latlng_is_valid() {
    assert!(LatLng::from_degrees(37.4249518, -122.082506).is_valid());
    assert!(LatLng::from_degrees(90.0, 180.0).is_valid());
    assert!(!LatLng::from_degrees(120.0, 200.0).is_valid());
    assert!(!LatLng::from_degrees(-100.0, -360.0).is_valid());
}
