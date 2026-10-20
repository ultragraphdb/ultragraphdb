/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Cap construction, radius, AddPoint/AddCap and CellUnionBound, against Go.

use super::cellid_tests::{CapJson, same_cap};
use super::golden::{Checker, f, ids, load, pt};
use serde::Deserialize;
use ultragraphdb_core_types::{Angle, Cap, ChordAngle};

#[derive(Deserialize)]
struct Case {
    center: [String; 3],
    angle: String,
    cap: CapJson,
    cell_union_bound: Vec<String>,
    points: Vec<[String; 3]>,
    contains_point: Vec<bool>,
    add_point: Vec<CapJson>,
    other: CapJson,
    add_cap: CapJson,
    complement: CapJson,
}

#[test]
fn caps_match_go() {
    let cases: Vec<Case> = load("caps.json");
    let mut c = Checker::new("caps");
    for (n, k) in cases.iter().enumerate() {
        let cap = Cap::from_center_angle(pt(&k.center), Angle::from_radians(f(&k.angle)));
        c.check(same_cap(cap, &k.cap), || format!("cap {n}: {cap:?}"));
        c.check(ids(&cap.cell_union_bound()) == k.cell_union_bound, || {
            format!("cap {n}: cell union bound")
        });
        let mut acc = cap;
        for (i, p) in k.points.iter().enumerate() {
            let q = pt(p);
            c.check(cap.contains_point(q) == k.contains_point[i], || {
                format!("cap {n}: contains point {i}")
            });
            acc = acc.add_point(q);
            c.check(same_cap(acc, &k.add_point[i]), || {
                format!("cap {n}: add point {i}: {acc:?}")
            });
        }
        let other = Cap::from_center_chord_angle(
            pt(&k.other.center),
            ChordAngle::from_squared_length(f(&k.other.chord)),
        );
        c.check(same_cap(other, &k.other), || {
            format!("cap {n}: other {other:?}")
        });
        c.check(same_cap(cap.add_cap(other), &k.add_cap), || {
            format!("cap {n}: add cap {:?}", cap.add_cap(other))
        });
        c.check(same_cap(cap.complement(), &k.complement), || {
            format!("cap {n}: complement {:?}", cap.complement())
        });
    }
    c.finish();
}
