/*
 * Copyright 2015 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/matrix3x3.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::point::{Point, ortho};
use crate::s2::r3::R3Vector;

/// A traditional 3x3 matrix of floating point values, used to convert between frames.
pub(crate) type Matrix3x3 = [[f64; 3]; 3];

/// Multiplies the matrix by the given value, as golang/geo's `matrix3x3.mul`.
fn mul(m: &Matrix3x3, p: Point) -> Point {
    Point(R3Vector::new(
        m[0][0] * p.0.x + m[0][1] * p.0.y + m[0][2] * p.0.z,
        m[1][0] * p.0.x + m[1][1] * p.0.y + m[1][2] * p.0.z,
        m[2][0] * p.0.x + m[2][1] * p.0.y + m[2][2] * p.0.z,
    ))
}

/// Returns the orthonormal frame for the given point on the unit sphere.
pub(crate) fn get_frame(p: Point) -> Matrix3x3 {
    // Given the point p on the unit sphere, extend this into a right-handed
    // coordinate frame of unit-length column vectors m = (x,y,z).  Note that
    // the vectors (x,y) are an orthonormal frame for the tangent space at point p,
    // while p itself is an orthonormal frame for the normal space at p.
    let (z, y) = (p.0, ortho(p).0);
    let x = y.cross(z);
    [[x.x, y.x, z.x], [x.y, y.y, z.y], [x.z, y.z, z.z]]
}

/// Returns the coordinates of the given point in standard axis-aligned basis from its
/// orthonormal basis m. The resulting point p satisfies the identity (p == fromFrame(m, q)).
pub(crate) fn from_frame(m: &Matrix3x3, q: Point) -> Point {
    mul(m, q)
}
