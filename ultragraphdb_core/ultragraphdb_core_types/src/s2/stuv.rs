/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/stuv.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Conversions between the (x,y,z), (face,u,v), (face,s,t) and (face,si,ti) coordinate
//! systems, using the quadratic projection.

use crate::s2::cellid::MAX_SIZE;
use crate::s2::point::Point;
use crate::s2::r3::{Axis, R3Vector};
use crate::s2::util::clamp;

/// The maximum value of an si- or ti-coordinate. It is one shift more than MaxSize.
pub(crate) const MAX_SI_TI: u64 = (MAX_SIZE as u64) << 1;

/// Converts an si- or ti-value to the corresponding s- or t-value.
/// Value is capped at 1.0 because there is no DCHECK in Go.
pub(crate) fn si_ti_to_st(si: u32) -> f64 {
    if si as u64 > MAX_SI_TI {
        return 1.0;
    }
    si as f64 / MAX_SI_TI as f64
}

/// Converts the s- or t-value to the corresponding u- or v-value. This is a non-linear
/// transformation from [-1,1] to [-1,1] that attempts to make the cell sizes more uniform.
/// This uses what the C++ version calls 'the quadratic transform'.
pub(crate) fn st_to_uv(s: f64) -> f64 {
    if s >= 0.5 {
        return (1.0 / 3.) * (4.0 * s * s - 1.0);
    }
    (1.0 / 3.) * (1.0 - 4.0 * (1.0 - s) * (1.0 - s))
}

/// The inverse of the stToUV transformation. Note that it is not always true that
/// stToUV(uvToST(x)) == x due to numerical errors.
pub(crate) fn uv_to_st(u: f64) -> f64 {
    if u >= 0.0 {
        return 0.5 * (1.0 + 3.0 * u).sqrt();
    }
    1.0 - 0.5 * (1.0 - 3.0 * u).sqrt()
}

/// Returns face ID from 0 to 5 containing the r. For points on the boundary between faces,
/// the result is arbitrary but deterministic.
pub(crate) fn face(r: R3Vector) -> usize {
    match r.largest_component() {
        Axis::X => {
            if r.x < 0.0 {
                3
            } else {
                0
            }
        }
        Axis::Y => {
            if r.y < 0.0 {
                4
            } else {
                1
            }
        }
        Axis::Z => {
            if r.z < 0.0 {
                5
            } else {
                2
            }
        }
    }
}

/// Converts the i- or j-index of a leaf cell to the minimum corresponding s- or t-value
/// contained by that cell.
pub(crate) fn ij_to_st_min(i: i64) -> f64 {
    i as f64 / MAX_SIZE as f64
}

/// Converts value in ST coordinates to a value in IJ coordinates.
pub(crate) fn st_to_ij(s: f64) -> i64 {
    clamp((MAX_SIZE as f64 * s).floor() as i64, 0, MAX_SIZE - 1)
}

/// Returns (u,v) for a given r and face. Unlike faceXYZToUV, it does not check that r is on
/// the given face.
pub(crate) fn valid_face_xyz_to_uv(face: usize, r: R3Vector) -> (f64, f64) {
    match face {
        0 => (r.y / r.x, r.z / r.x),
        1 => (-r.x / r.y, r.z / r.y),
        2 => (-r.x / r.z, -r.y / r.z),
        3 => (r.z / r.x, r.y / r.x),
        4 => (r.z / r.y, -r.x / r.y),
        _ => (-r.y / r.z, -r.x / r.z),
    }
}

/// Converts a direction vector (not necessarily unit length) to (face, u, v) coordinates.
pub(crate) fn xyz_to_face_uv(r: R3Vector) -> (usize, f64, f64) {
    let f = face(r);
    let (u, v) = valid_face_xyz_to_uv(f, r);
    (f, u, v)
}

/// Turns face and UV coordinates into an unnormalized 3 vector.
pub(crate) fn face_uv_to_xyz(face: usize, u: f64, v: f64) -> R3Vector {
    match face {
        0 => R3Vector::new(1.0, u, v),
        1 => R3Vector::new(-u, 1.0, v),
        2 => R3Vector::new(-u, -v, 1.0),
        3 => R3Vector::new(-1.0, -v, -u),
        4 => R3Vector::new(v, -1.0, -u),
        _ => R3Vector::new(v, u, -1.0),
    }
}

/// Returns the u and v values (which may lie outside the range [-1, 1]) if the dot product
/// of the point p with the given face normal is positive.
pub(crate) fn face_xyz_to_uv(face: usize, p: Point) -> Option<(f64, f64)> {
    let ok = match face {
        0 => p.0.x > 0.0,
        1 => p.0.y > 0.0,
        2 => p.0.z > 0.0,
        3 => p.0.x < 0.0,
        4 => p.0.y < 0.0,
        _ => p.0.z < 0.0,
    };
    if !ok {
        return None;
    }
    Some(valid_face_xyz_to_uv(face, p.0))
}

/// Transforms the given point P to the (u,v,w) coordinate frame of the given face where the
/// w-axis represents the face normal.
pub(crate) fn face_xyz_to_uvw(face: usize, p: Point) -> Point {
    // The result coordinates are simply the dot products of P with the (u,v,w)
    // axes for the given face (see faceUVWAxes).
    let v = p.0;
    Point(match face {
        0 => R3Vector::new(v.y, v.z, v.x),
        1 => R3Vector::new(-v.x, v.z, v.y),
        2 => R3Vector::new(-v.x, -v.y, v.z),
        3 => R3Vector::new(-v.z, -v.y, -v.x),
        4 => R3Vector::new(-v.z, v.x, -v.y),
        _ => R3Vector::new(v.y, v.x, -v.z),
    })
}

/// Transforms the (si, ti) coordinates to a (not necessarily unit length) Point on the given
/// face.
pub(crate) fn face_si_ti_to_xyz(face: usize, si: u32, ti: u32) -> Point {
    Point(face_uv_to_xyz(
        face,
        st_to_uv(si_ti_to_st(si)),
        st_to_uv(si_ti_to_st(ti)),
    ))
}

/// The U, V, and W axes for each face.
const FACE_UVW_AXES: [[R3Vector; 3]; 6] = [
    [
        R3Vector::new(0.0, 1.0, 0.0),
        R3Vector::new(0.0, 0.0, 1.0),
        R3Vector::new(1.0, 0.0, 0.0),
    ],
    [
        R3Vector::new(-1.0, 0.0, 0.0),
        R3Vector::new(0.0, 0.0, 1.0),
        R3Vector::new(0.0, 1.0, 0.0),
    ],
    [
        R3Vector::new(-1.0, 0.0, 0.0),
        R3Vector::new(0.0, -1.0, 0.0),
        R3Vector::new(0.0, 0.0, 1.0),
    ],
    [
        R3Vector::new(0.0, 0.0, -1.0),
        R3Vector::new(0.0, -1.0, 0.0),
        R3Vector::new(-1.0, 0.0, 0.0),
    ],
    [
        R3Vector::new(0.0, 0.0, -1.0),
        R3Vector::new(1.0, 0.0, 0.0),
        R3Vector::new(0.0, -1.0, 0.0),
    ],
    [
        R3Vector::new(0.0, 1.0, 0.0),
        R3Vector::new(1.0, 0.0, 0.0),
        R3Vector::new(0.0, 0.0, -1.0),
    ],
];

/// Returns the given axis of the given face. A face past 5, of an invalid cell id, reads as face 5, as the default
/// arms of the other face functions read it; Go indexes past the table and panics.
fn uvw_axis(face: usize, axis: usize) -> R3Vector {
    FACE_UVW_AXES.get(face).unwrap_or(&FACE_UVW_AXES[5])[axis]
}

/// Returns the u-axis for the given face.
pub(crate) fn u_axis(face: usize) -> R3Vector {
    uvw_axis(face, 0)
}

/// Returns the v-axis for the given face.
pub(crate) fn v_axis(face: usize) -> R3Vector {
    uvw_axis(face, 1)
}
