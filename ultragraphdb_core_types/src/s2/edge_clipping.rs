/*
 * Copyright 2017 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/edge_clipping.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Clipping edges to cube faces and to rectangles in (u,v)-space. The subset golang/geo's
//! ShapeIndex and Loop use.

use crate::s2::gomath;
use crate::s2::point::Point;
use crate::s2::r2::{R2Point, R2Rect};
use crate::s2::r3::R3Vector;
use crate::s2::stuv::{face, face_xyz_to_uvw, valid_face_xyz_to_uv};

// Go derives its clipping tolerances from edgeClipErrorUVCoord = 2.25 * dblEpsilon and
// faceClipErrorUVCoord = 9.0 * (1.0 / math.Sqrt2) * dblEpsilon; the constant expressions this
// port needs are evaluated exactly as Go does and given by their bits where they are used.

/// Go's constant `1 - faceClipErrorUVCoord`, rounded once from the exact difference.
const MAX_SAFE_UV_COORD: f64 = f64::from_bits(0x3feffffffffffff3);

/// Returns the (u,v) coordinates for the portion of the edge AB that intersects the given
/// face, but rather than clipping to the square [-1,1]x[-1,1] in (u,v) space, this method
/// clips to [-R,R]x[-R,R] where R=(1+padding). Padding must be non-negative.
pub(crate) fn clip_to_padded_face(
    a: Point,
    b: Point,
    f: usize,
    padding: f64,
) -> Option<(R2Point, R2Point)> {
    // Fast path: both endpoints are on the given face.
    if face(a.0) == f && face(b.0) == f {
        let (au, av) = valid_face_xyz_to_uv(f, a.0);
        let (bu, bv) = valid_face_xyz_to_uv(f, b.0);
        return Some((R2Point::new(au, av), R2Point::new(bu, bv)));
    }

    // Convert everything into the (u,v,w) coordinates of the given face. Note
    // that the cross product *must* be computed in the original (x,y,z)
    // coordinate system because PointCross (unlike the mathematical cross
    // product) can produce different results in different coordinate systems
    // when one argument is a linear multiple of the other, due to the use of
    // symbolic perturbations.
    let mut norm_uvw = face_xyz_to_uvw(f, a.point_cross(b)).0;
    let a_uvw = face_xyz_to_uvw(f, a).0;
    let b_uvw = face_xyz_to_uvw(f, b).0;

    // Padding is handled by scaling the u- and v-components of the normal.
    // Letting R=1+padding, this means that when we compute the dot product of
    // the normal with a cube face vertex (such as (-1,-1,1)), we will actually
    // compute the dot product with the scaled vertex (-R,-R,1). This allows
    // methods such as intersectsFace, exitAxis, etc, to handle padding
    // with no further modifications.
    let scale_uv = 1.0 + padding;
    let scaled_n = R3Vector::new(scale_uv * norm_uvw.x, scale_uv * norm_uvw.y, norm_uvw.z);
    if !intersects_face(scaled_n) {
        return None;
    }

    // TODO(roberts): This is a workaround for extremely small vectors where some
    // loss of precision can occur in Normalize causing underflow. When PointCross
    // is updated to work around this, this can be removed.
    if gomath::max(
        norm_uvw.x.abs(),
        gomath::max(norm_uvw.y.abs(), norm_uvw.z.abs()),
    ) < gomath::ldexp(1.0, -511)
    {
        norm_uvw = norm_uvw.mul(gomath::ldexp(1.0, 563));
    }

    let norm_uvw = norm_uvw.normalize();

    let a_tan = norm_uvw.cross(a_uvw);
    let b_tan = b_uvw.cross(norm_uvw);

    // As described in clipDestination, if the sum of the scores from clipping the two
    // endpoints is 3 or more, then the segment does not intersect this face.
    let (a_uv, a_score) =
        clip_destination(b_uvw, a_uvw, scaled_n.mul(-1.0), b_tan, a_tan, scale_uv);
    let (b_uv, b_score) = clip_destination(a_uvw, b_uvw, scaled_n, a_tan, b_tan, scale_uv);

    if a_score + b_score < 3 {
        Some((a_uv, b_uv))
    } else {
        None
    }
}

/// Reports whether a given directed line L intersects the cube face F. The line L is
/// defined by its normal N in the (u,v,w) coordinates of F.
fn intersects_face(n: R3Vector) -> bool {
    // L intersects the [-1,1]x[-1,1] square in (u,v) if and only if the dot
    // products of N with the four corner vertices (-1,-1,1), (1,-1,1), (1,1,1),
    // and (-1,1,1) do not all have the same sign. This is true exactly when
    // |Nu| + |Nv| >= |Nw|. The code below evaluates this expression exactly.
    let u = n.x.abs();
    let v = n.y.abs();
    let w = n.z.abs();

    // We only need to consider the cases where u or v is the smallest value,
    // since if w is the smallest then both expressions below will have a
    // positive LHS and a negative RHS.
    (v >= w - u) && (u >= w - v)
}

/// Reports whether a directed line L intersects two opposite edges of a cube face F. This
/// includes the case where L passes exactly through a corner vertex of F. The directed line
/// L is defined by its normal N in the (u,v,w) coordinates of F.
fn intersects_opposite_edges(n: R3Vector) -> bool {
    // The line L intersects opposite edges of the [-1,1]x[-1,1] (u,v) square if
    // and only exactly two of the corner vertices lie on each side of L. This
    // is true exactly when ||Nu| - |Nv|| >= |Nw|. The code below evaluates this
    // expression exactly.
    let u = n.x.abs();
    let v = n.y.abs();
    let w = n.z.abs();

    // If w is the smallest, the following line returns an exact result.
    if (u - v).abs() != w {
        return (u - v).abs() >= w;
    }

    // Otherwise u - v = w exactly, or w is not the smallest value. In either
    // case the following returns the correct result.
    if u >= v {
        return u - w >= v;
    }
    v - w >= u
}

/// The axis of exit: U or V.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExitAxis {
    U,
    V,
}

/// Returns the axis that the directed line L exits the cube face F. The line L is
/// represented by its CCW normal N in the (u,v,w) coordinates of F. It returns axisU if L
/// exits through the u=-1 or u=+1 edge, and axisV if L exits through the v=-1 or v=+1 edge.
/// Either result is acceptable if L exits exactly through a corner vertex of the cube face.
fn exit_axis(n: R3Vector) -> ExitAxis {
    if intersects_opposite_edges(n) {
        // The line passes through opposite edges of the face.
        // It exits through the v=+1 or v=-1 edge if the u-component of N has a
        // larger absolute magnitude than the v-component.
        if n.x.abs() >= n.y.abs() {
            return ExitAxis::V;
        }
        return ExitAxis::U;
    }

    // The line passes through two adjacent edges of the face.
    // It exits the v=+1 or v=-1 edge if an even number of the components of N
    // are negative. We test this using signbit() rather than multiplication
    // to avoid the possibility of underflow.
    let x = n.x.is_sign_negative() as u8;
    let y = n.y.is_sign_negative() as u8;
    let z = n.z.is_sign_negative() as u8;

    if x ^ y ^ z == 0 {
        return ExitAxis::V;
    }
    ExitAxis::U
}

/// Returns the UV coordinates of the point where a directed line L (represented by the CCW
/// normal of this point), exits the cube face this point is derived from along the given
/// axis.
fn exit_point(n: R3Vector, a: ExitAxis) -> R2Point {
    if a == ExitAxis::U {
        let u = if n.y > 0.0 { 1.0 } else { -1.0 };
        return R2Point::new(u, (-u * n.x - n.z) / n.y);
    }

    let v = if n.x < 0.0 { 1.0 } else { -1.0 };
    R2Point::new((-v * n.y - n.z) / n.x, v)
}

/// Returns a score which is used to indicate if the clipped edge AB on the given face
/// intersects the face at all. This function returns the score for the given endpoint, which
/// is an integer ranging from 0 to 3. If the sum of the scores from both of the endpoints is
/// 3 or more, then AB does not intersect this cube face.
///
/// Note that the bound is computed by intersecting the bound from a point with the given face
/// rather than computing it directly in uvw.
fn clip_destination(
    a: R3Vector,
    b: R3Vector,
    scaled_n: R3Vector,
    a_tan: R3Vector,
    b_tan: R3Vector,
    scale_uv: f64,
) -> (R2Point, i32) {
    // Optimization: if B is within the safe region of the face, use it.
    if b.z > 0.0 {
        let uv = R2Point::new(b.x / b.z, b.y / b.z);
        if gomath::max(uv.x.abs(), uv.y.abs()) <= MAX_SAFE_UV_COORD {
            return (uv, 0);
        }
    }

    // Otherwise find the point B' where the line AB exits the face.
    let mut uv = exit_point(scaled_n, exit_axis(scaled_n)).mul(scale_uv);

    let p = R3Vector::new(uv.x, uv.y, 1.0);

    // Determine if the exit point B' is contained within the segment. We do this
    // by computing the dot products with two inward-facing tangent vectors at A
    // and B. If either dot product is negative, we say that B' is on the "wrong
    // side" of that point. As the point B' moves around the great circle AB past
    // the segment endpoint B, it is initially on the wrong side of B only; as it
    // moves further it is on the wrong side of both endpoints; and then it is on
    // the wrong side of A only. If the exit point B' is on the wrong side of
    // either endpoint, we can't use it; instead the segment is clipped at the
    // original endpoint B.
    //
    // We reject the segment if the sum of the scores of the two endpoints is 3
    // or more. Here is what that rule encodes:
    //  - If B' is on the wrong side of A, then the other clipped endpoint A'
    //    must be in the interior of AB (otherwise AB' would go the wrong way
    //    around the circle). There is a similar rule for A'.
    //  - If B' is on the wrong side of either endpoint (and therefore we must
    //    use the original endpoint B instead), then it must be possible to
    //    project B onto this face (i.e., its w-coordinate must be positive).
    //    This rule is only necessary to handle certain zero-length edges (A=B).
    let mut score = 0;
    if p.sub(a).dot(a_tan) < 0.0 {
        score = 2; // B' is on wrong side of A.
    } else if p.sub(b).dot(b_tan) < 0.0 {
        score = 1; // B' is on wrong side of B.
    }

    if score > 0 {
        // B' is not in the interior of AB.
        if b.z <= 0.0 {
            score = 3; // B cannot be projected onto this face.
        } else {
            uv = R2Point::new(b.x / b.z, b.y / b.z);
        }
    }

    (uv, score)
}

/// Reports whether the edge defined by AB intersects the given closed rectangle to within
/// the error bound.
pub(crate) fn edge_intersects_rect(a: R2Point, b: R2Point, r: R2Rect) -> bool {
    // First check whether the bounds of a Rect around AB intersects the given rect.
    if !r.intersects(R2Rect::from_points(&[a, b])) {
        return false;
    }

    // Otherwise AB intersects the rect if and only if all four vertices of rect
    // do not lie on the same side of the extended line AB. We test this by finding
    // the two vertices of rect with minimum and maximum projections onto the normal
    // of AB, and computing their dot products with the edge normal.
    let n = b.sub(a).ortho();

    let i = if n.x >= 0.0 { 1 } else { 0 };
    let j = if n.y >= 0.0 { 1 } else { 0 };

    let max = n.dot(r.vertex_ij(i, j).sub(a));
    let min = n.dot(r.vertex_ij(1 - i, 1 - j).sub(a));

    (max >= 0.0) && (min <= 0.0)
}

/// Returns the value b1 such that the line through (a, a1) and (b, b1) passes through
/// (x, b1). The interpolation is done in a way that keeps the error small when the line is
/// steep.
pub(crate) fn interpolate_float64(x: f64, a: f64, b: f64, a1: f64, b1: f64) -> f64 {
    // To get results that are accurate near both A and B, we interpolate
    // starting from the closer of the two points.
    if a == b {
        return a1;
    }
    if (a - x).abs() <= (b - x).abs() {
        return a1 + (b1 - a1) * (x - a) / (b - a);
    }
    b1 + (a1 - b1) * (x - b) / (a - b)
}
