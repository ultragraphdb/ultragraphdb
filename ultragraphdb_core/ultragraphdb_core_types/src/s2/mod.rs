/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A port of the part of golang/geo v0.0.0-20260427214057-41a1a8c7eb2a that Dgraph's geo
//! index uses. Each sub-module is a translation of the golang/geo file (or package) of the
//! same name; `gomath` translates the Go `math` functions golang/geo calls.

mod cap;
mod cell;
mod cellid;
mod cellunion;
mod edge_clipping;
mod edge_crosser;
mod edge_crossings;
mod gomath;
mod latlng;
mod r#loop;
mod matrix3x3;
mod metric;
mod paddedcell;
mod point;
mod predicates;
mod r1;
mod r2;
mod r3;
mod rect;
mod rect_bounder;
mod region;
mod regioncoverer;
mod s1;
mod shape;
mod shapeindex;
mod shapeutil;
mod stuv;
mod util;

pub use cap::Cap;
pub use cell::Cell;
pub use cellid::{CellId, FACE_BITS, MAX_LEVEL, NUM_FACES, POS_BITS};
pub use cellunion::CellUnion;
pub use edge_crosser::EdgeCrosser;
pub use edge_crossings::{
    Crossing, angle_contains_vertex, crossing_sign, edge_or_vertex_crossing, vertex_crossing,
};
pub use latlng::LatLng;
pub use r#loop::Loop;
pub use point::{Point, chord_angle_between_points, ordered_ccw};
pub use predicates::{Direction, robust_sign, sign};
pub use r1::R1Interval;
pub use r2::{R2Point, R2Rect};
pub use r3::R3Vector;
pub use rect::Rect;
pub use rect_bounder::{RectBounder, expand_for_subregions};
pub use region::Region;
pub use regioncoverer::RegionCoverer;
pub use s1::{Angle, ChordAngle, S1Interval};

/// Go's `math.Sin`, as golang/geo computes it.
pub fn go_sin(x: f64) -> f64 {
    gomath::sin(x)
}

/// Go's `math.Cos`.
pub fn go_cos(x: f64) -> f64 {
    gomath::cos(x)
}

/// Go's `math.Atan2`.
pub fn go_atan2(y: f64, x: f64) -> f64 {
    gomath::atan2(y, x)
}

/// Go's `math.Asin`.
pub fn go_asin(x: f64) -> f64 {
    gomath::asin(x)
}

/// Go's `math.Remainder`.
pub fn go_remainder(x: f64, y: f64) -> f64 {
    gomath::remainder(x, y)
}

/// Go's `math.Ldexp`.
pub fn go_ldexp(frac: f64, exp: i64) -> f64 {
    gomath::ldexp(frac, exp)
}

/// Go's `math.Frexp`.
pub fn go_frexp(f: f64) -> (f64, i64) {
    gomath::frexp(f)
}

/// Go's `math.Ilogb`.
pub fn go_ilogb(x: f64) -> i64 {
    gomath::ilogb(x)
}

/// Go's `math.Nextafter`.
pub fn go_nextafter(x: f64, y: f64) -> f64 {
    gomath::nextafter(x, y)
}

/// Go's `math.Max`.
pub fn go_max(x: f64, y: f64) -> f64 {
    gomath::max(x, y)
}

/// Go's `math.Min`.
pub fn go_min(x: f64, y: f64) -> f64 {
    gomath::min(x, y)
}
