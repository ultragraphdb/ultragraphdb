/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/point.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::gomath;
use crate::s2::latlng::LatLng;
use crate::s2::matrix3x3::{Matrix3x3, from_frame};
use crate::s2::predicates::{Direction, robust_sign};
use crate::s2::r3::{Axis, R3Vector};
use crate::s2::s1::{Angle, ChordAngle};

/// A point on the unit sphere as a normalized 3D vector. Use one of the constructors.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point(pub(crate) R3Vector);

impl Point {
    /// Wraps the vector as a point without normalizing it, as Go's `s2.Point{v}`.
    pub const fn from_vector(v: R3Vector) -> Point {
        Point(v)
    }

    /// Creates a new normalized point from coordinates. This always returns a valid point:
    /// if the given coordinates can not be normalized the origin point is returned.
    pub fn from_coords(x: f64, y: f64, z: f64) -> Point {
        if x == 0.0 && y == 0.0 && z == 0.0 {
            return Point::origin();
        }
        Point(R3Vector::new(x, y, z).normalize())
    }

    /// Returns the unique "origin" on the sphere for operations that need a fixed reference
    /// point; the "point at infinity" used for point-in-polygon testing.
    pub const fn origin() -> Point {
        Point(R3Vector::new(
            f64::from_bits(0xbf847a99aa86ed3b), // -0.0099994664350250197
            f64::from_bits(0x3f653cc5488bec88), // 0.0025924542609324121
            f64::from_bits(0x3fefff901a72d2ac), // 0.99994664350250195
        ))
    }

    /// Returns a Point for the given LatLng.
    pub fn from_lat_lng(ll: LatLng) -> Point {
        let phi = ll.lat.radians();
        let theta = ll.lng.radians();
        let cosphi = gomath::cos(phi);
        Point(R3Vector::new(
            gomath::cos(theta) * cosphi,
            gomath::sin(theta) * cosphi,
            gomath::sin(phi),
        ))
    }

    /// The vector of this point.
    pub fn vector(&self) -> R3Vector {
        self.0
    }

    /// The x coordinate.
    pub fn x(&self) -> f64 {
        self.0.x
    }

    /// The y coordinate.
    pub fn y(&self) -> f64 {
        self.0.y
    }

    /// The z coordinate.
    pub fn z(&self) -> f64 {
        self.0.z
    }

    /// Returns a Point that is orthogonal to both p and op. This is similar to p.Cross(op)
    /// (the true cross product) except that it does a better job of ensuring orthogonality
    /// when the Point is nearly parallel to op, it returns a non-zero result even when
    /// p == op or p == -op and the result is a Point.
    pub fn point_cross(&self, op: Point) -> Point {
        // NOTE(dnadasi): In the C++ API the equivalent method here was known as "RobustCrossProd",
        // but PointCross more accurately describes how this method is used.
        let x = self.0.add(op.0).cross(op.0.sub(self.0));

        // Compare exactly to the 0 vector.
        if x == R3Vector::default() {
            // The only result that makes sense mathematically is to return zero, but
            // we find it more convenient to return an arbitrary orthogonal vector.
            return Point(self.0.ortho());
        }

        Point(x)
    }

    /// Returns the angle between two points.
    pub fn distance(&self, b: Point) -> Angle {
        self.0.angle(b.0)
    }

    /// Reports whether the two points are similar enough to be equal.
    pub fn approx_equal(&self, other: Point) -> bool {
        self.approx_equal_within(other, Angle(1e-15))
    }

    /// Reports whether the two points are within the given epsilon.
    pub(crate) fn approx_equal_within(&self, other: Point, eps: Angle) -> bool {
        self.0.angle(other.0) <= eps
    }

    /// Returns a unit-length vector to use as the reference direction for deciding whether a
    /// polygon with semi-open boundaries contains the given vertex "a". The result is unit
    /// length and is guaranteed to be different from the given point "a".
    pub(crate) fn reference_dir(&self) -> Point {
        ortho(*self)
    }
}

/// Returns true if the edges OA, OB, and OC are encountered in that order while sweeping CCW
/// around the point O.
///
/// You can think of this as testing whether A <= B <= C with respect to the CCW ordering
/// around O that starts at A, or equivalently, whether B is contained in the range of angles
/// (inclusive) that starts at A and extends CCW to C. Properties:
///
///  (1) If OrderedCCW(a,b,c,o) && OrderedCCW(b,a,c,o), then a == b
///  (2) If OrderedCCW(a,b,c,o) && OrderedCCW(a,c,b,o), then b == c
///  (3) If OrderedCCW(a,b,c,o) && OrderedCCW(c,b,a,o), then a == b == c
///  (4) If a == b or b == c, then OrderedCCW(a,b,c,o) is true
///  (5) Otherwise if a == c, then OrderedCCW(a,b,c,o) is false
pub fn ordered_ccw(a: Point, b: Point, c: Point, o: Point) -> bool {
    let mut sum = 0;
    if robust_sign(b, o, a) != Direction::Clockwise {
        sum += 1;
    }
    if robust_sign(c, o, b) != Direction::Clockwise {
        sum += 1;
    }
    if robust_sign(a, o, c) == Direction::CounterClockwise {
        sum += 1;
    }
    sum >= 2
}

/// Constructs a ChordAngle corresponding to the distance between the two given points.
/// The points must be unit length.
pub fn chord_angle_between_points(x: Point, y: Point) -> ChordAngle {
    ChordAngle(gomath::min(4.0, x.0.sub(y.0).norm2()))
}

/// Generates a slice of points shaped as a regular polygon with numVertices vertices, all on
/// a circle of the specified angular radius around the center. The radius is the actual
/// distance from the center to each vertex.
pub(crate) fn regular_points_for_frame(
    frame: &Matrix3x3,
    radius: Angle,
    num_vertices: usize,
) -> Vec<Point> {
    // We construct the loop in the given frame coordinates, with the center at
    // (0, 0, 1). For a loop of radius r, the loop vertices have the form
    // (x, y, z) where x^2 + y^2 = sin(r) and z = cos(r). The distance on the
    // sphere (arc length) from each vertex to the center is acos(cos(r)) = r.
    let z = gomath::cos(radius.radians());
    let r = gomath::sin(radius.radians());
    let radian_step = 2.0 * core::f64::consts::PI / num_vertices as f64;
    (0..num_vertices)
        .map(|i| {
            let angle = i as f64 * radian_step;
            let p = Point(R3Vector::new(
                r * gomath::cos(angle),
                r * gomath::sin(angle),
                z,
            ));
            Point(from_frame(frame, p).0.normalize())
        })
        .collect()
}

/// Returns a unit-length vector that is orthogonal to "a". Satisfies Ortho(-a) = -Ortho(a)
/// for all a.
///
/// Note that Vector3 also defines an "Ortho" method, but this one is preferred for use in S2
/// code because it explicitly tries to avoid result coordinates that are zero. (This is a
/// performance optimization that reduces the amount of time spent in functions that handle
/// degeneracies.)
pub(crate) fn ortho(a: Point) -> Point {
    let mut temp = R3Vector::new(0.012, 0.0053, 0.00457);
    match a.0.largest_component() {
        Axis::X => temp.z = 1.0,
        Axis::Y => temp.x = 1.0,
        Axis::Z => temp.y = 1.0,
    }
    Point(a.0.cross(temp).normalize())
}
