/*
 * Copyright 2016 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/predicates.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Predicates that are guaranteed to produce correct, consistent results. They are also
//! relatively efficient. This is achieved by computing conservative error bounds and falling
//! back to high precision or even exact arithmetic when the result is uncertain. Such
//! predicates are useful in implementing robust algorithms.
//!
//! See also EdgeCrosser, which implements various exact edge-crossing predicates more
//! efficiently than can be done here.

use crate::s2::point::Point;
use crate::s2::r3::{PreciseVector, R3Vector};
use core::ops::{Mul, Neg};

/// The C++ DBL_EPSILON equivalent.
const MACHINE_EPSILON: f64 = f64::EPSILON;

/// The C++ DBL_MIN equivalent.
const SMALLEST_NORMAL: f64 = f64::MIN_POSITIVE;

/// The maximum error in computing (AxB).C where all vectors are unit length; Go's constant
/// `1.8274 * machineEpsilon64`.
const MAX_DETERMINANT_ERROR: f64 = 1.8274 * MACHINE_EPSILON;

/// The factor to scale the magnitudes by when checking for the sign of set of points with
/// certainty; Go's constant `3.2321 * machineEpsilon64`.
const DET_ERROR_MULTIPLIER: f64 = 3.2321 * MACHINE_EPSILON;

/// An indication of the ordering of a set of points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// The points are in clockwise order.
    Clockwise = -1,
    /// The points are collinear or the order could not be determined.
    Indeterminate = 0,
    /// The points are in counter-clockwise order.
    CounterClockwise = 1,
}

impl Direction {
    /// The direction as Go's int value: -1, 0 or +1.
    pub fn value(&self) -> i32 {
        *self as i32
    }

    fn from_sign(s: i32) -> Direction {
        match s.signum() {
            -1 => Direction::Clockwise,
            0 => Direction::Indeterminate,
            _ => Direction::CounterClockwise,
        }
    }
}

impl Neg for Direction {
    type Output = Direction;

    fn neg(self) -> Direction {
        Direction::from_sign(-self.value())
    }
}

impl Mul for Direction {
    type Output = Direction;

    fn mul(self, rhs: Direction) -> Direction {
        Direction::from_sign(self.value() * rhs.value())
    }
}

/// Returns true if the points A, B, C are strictly counterclockwise, and returns false if
/// the points are clockwise or collinear (i.e. if they are all contained on some great
/// circle).
///
/// Due to numerical errors, situations may arise that are mathematically impossible, e.g. ABC
/// may be considered strictly CCW while BCA is not. However, the implementation guarantees
/// the following:
///
/// If Sign(a,b,c), then !Sign(c,b,a) for all a,b,c.
pub fn sign(a: Point, b: Point, c: Point) -> bool {
    // NOTE(dnadasi): In the C++ API the equivalent method here was known as "SimpleSign".

    // We compute the signed volume of the parallelepiped ABC. The usual
    // formula for this is (A ⨯ B) · C, but we compute it here using (C ⨯ A) · B
    // in order to ensure that ABC and CBA are not both CCW. This follows
    // from the following identities (which are true numerically, not just
    // mathematically):
    //
    //     (1) x ⨯ y == -(y ⨯ x)
    //     (2) -x · y == -(x · y)
    c.0.cross(a.0).dot(b.0) > 0.0
}

/// Returns a Direction representing the ordering of the points. CounterClockwise is returned
/// if the points are in counter-clockwise order, Clockwise for clockwise, and Indeterminate
/// if any two points are the same (collinear), or the sign could not completely be
/// determined.
///
/// This function has additional logic to make sure that the above properties hold even when
/// the three points are coplanar, and to deal with the limitations of floating-point
/// arithmetic.
///
/// RobustSign satisfies the following conditions:
///
///  (1) RobustSign(a,b,c) == Indeterminate if and only if a == b, b == c, or c == a
///  (2) RobustSign(b,c,a) == RobustSign(a,b,c) for all a,b,c
///  (3) RobustSign(c,b,a) == -RobustSign(a,b,c) for all a,b,c
pub fn robust_sign(a: Point, b: Point, c: Point) -> Direction {
    let sign = triage_sign(a, b, c);
    if sign == Direction::Indeterminate {
        return expensive_sign(a, b, c);
    }
    sign
}

/// Reports the direction sign of the points in a numerically stable way. Unlike triageSign,
/// this method can usually compute the correct determinant sign even when all three points
/// are as collinear as possible.
fn stable_sign(a: Point, b: Point, c: Point) -> Direction {
    let ab = b.0.sub(a.0);
    let ab2 = ab.norm2();
    let bc = c.0.sub(b.0);
    let bc2 = bc.norm2();
    let ca = a.0.sub(c.0);
    let ca2 = ca.norm2();

    // Now compute the determinant ((A-C)x(B-C)).C, where the vertices have been
    // cyclically permuted if necessary so that AB is the longest edge. (This
    // minimizes the magnitude of cross product.)  At the same time we also
    // compute the maximum error in the determinant.

    // The two shortest edges, pointing away from their common point.
    let (e1, e2, op): (R3Vector, R3Vector, R3Vector) = if ab2 >= bc2 && ab2 >= ca2 {
        // AB is the longest edge.
        (ca, bc, c.0)
    } else if bc2 >= ca2 {
        // BC is the longest edge.
        (ab, ca, a.0)
    } else {
        // CA is the longest edge.
        (bc, ab, b.0)
    };

    let det = -e1.cross(e2).dot(op);
    let max_err = DET_ERROR_MULTIPLIER * (e1.norm2() * e2.norm2()).sqrt();

    // Errors smaller than this value may not be accurate due to underflow.
    let min_no_underflow_error = DET_ERROR_MULTIPLIER * SMALLEST_NORMAL.sqrt();
    if max_err < min_no_underflow_error {
        return Direction::Indeterminate;
    }

    // If the determinant isn't zero, within maxErr, we know definitively the point ordering.
    if det > max_err {
        return Direction::CounterClockwise;
    }
    if det < -max_err {
        return Direction::Clockwise;
    }
    Direction::Indeterminate
}

/// Returns the direction sign of the points. It returns Indeterminate if two points are
/// identical or the result is uncertain. Uncertain cases can be resolved, if desired, by
/// calling expensiveSign.
pub(crate) fn triage_sign(a: Point, b: Point, c: Point) -> Direction {
    let det = a.0.cross(b.0).dot(c.0);
    if det > MAX_DETERMINANT_ERROR {
        return Direction::CounterClockwise;
    }
    if det < -MAX_DETERMINANT_ERROR {
        return Direction::Clockwise;
    }
    Direction::Indeterminate
}

/// Reports the direction sign of the points. It returns Indeterminate if two of the input
/// points are the same. It uses multiple-precision arithmetic to ensure that its results are
/// always self-consistent.
pub(crate) fn expensive_sign(a: Point, b: Point, c: Point) -> Direction {
    // Return Indeterminate if and only if two points are the same.
    // This ensures RobustSign(a,b,c) == Indeterminate if and only if a == b, b == c, or c == a.
    // ie. Property 1 of RobustSign.
    if a == b || b == c || c == a {
        return Direction::Indeterminate;
    }

    // Next we try recomputing the determinant still using floating-point
    // arithmetic but in a more precise way. This is more expensive than the
    // simple calculation done by triageSign, but it is still *much* cheaper
    // than using arbitrary-precision arithmetic. This optimization is able to
    // compute the correct determinant sign in virtually all cases except when
    // the three points are truly collinear (e.g., three points on the equator).
    let det_sign = stable_sign(a, b, c);
    if det_sign != Direction::Indeterminate {
        return det_sign;
    }

    // Otherwise fall back to exact arithmetic and symbolic permutations.
    exact_sign(a, b, c, true)
}

/// Reports the direction sign of the points computed using high-precision arithmetic and/or
/// symbolic perturbations.
fn exact_sign(a: Point, b: Point, c: Point, perturb: bool) -> Direction {
    // Sort the three points in lexicographic order, keeping track of the sign
    // of the permutation. (Each exchange inverts the sign of the determinant.)
    let mut perm_sign = Direction::CounterClockwise;
    let (mut pa, mut pb, mut pc) = (a, b, c);
    if pa.0.compare(pb.0) > 0 {
        core::mem::swap(&mut pa, &mut pb);
        perm_sign = -perm_sign;
    }
    if pb.0.compare(pc.0) > 0 {
        core::mem::swap(&mut pb, &mut pc);
        perm_sign = -perm_sign;
    }
    if pa.0.compare(pb.0) > 0 {
        core::mem::swap(&mut pa, &mut pb);
        perm_sign = -perm_sign;
    }

    // Construct multiple-precision versions of the sorted points and compute
    // their precise 3x3 determinant. A point that is not finite has none, where Go's big.Float panics.
    let (Some(xa), Some(xb), Some(xc)) = (
        PreciseVector::from_vector(pa.0),
        PreciseVector::from_vector(pb.0),
        PreciseVector::from_vector(pc.0),
    ) else {
        return Direction::Indeterminate;
    };
    let xb_cross_xc = xb.cross(&xc);
    let det = xa.dot(&xb_cross_xc);

    // The precision of big.Float is high enough that the result should always
    // be exact enough (no rounding was performed).

    // If the exact determinant is non-zero, we're done.
    let mut det_sign = Direction::from_sign(det.sign());
    if det_sign == Direction::Indeterminate && perturb {
        // Otherwise, we need to resort to symbolic perturbations to resolve the
        // sign of the determinant.
        det_sign = symbolically_perturbed_sign(&xa, &xb, &xc, &xb_cross_xc);
    }
    perm_sign * det_sign
}

/// Reports the sign of the determinant of three points A, B, C under a model where every
/// possible Point is slightly perturbed by a unique infinitesmal amount such that no three
/// perturbed points are collinear and no four points are coplanar. The perturbations are so
/// small that they do not change the sign of any determinant that was non-zero before the
/// perturbations, and therefore can be safely ignored unless the determinant of three points
/// is exactly zero (using multiple-precision arithmetic). This returns CounterClockwise or
/// Clockwise according to the sign of the determinant after the symbolic perturbations are
/// taken into account.
///
/// This requires that the 3x3 determinant of A, B, C must be exactly zero. And the points
/// must be distinct, with A < B < C in lexicographic order.
///
/// Reference: "Simulation of Simplicity" (Edelsbrunner and Muecke, ACM Transactions on
/// Graphics, 1990).
fn symbolically_perturbed_sign(
    a: &PreciseVector,
    b: &PreciseVector,
    c: &PreciseVector,
    b_cross_c: &PreciseVector,
) -> Direction {
    // The perturbations are chosen such that
    //
    //   da.Z > da.Y > da.X > db.Z > db.Y > db.X > dc.Z > dc.Y > dc.X
    //
    // where each perturbation is so much smaller than the previous one that we
    // don't even need to consider it unless the coefficients of all previous
    // perturbations are zero. The following code simply enumerates the
    // coefficients of the perturbations (and products of perturbations) that
    // appear in the determinant above, in order of decreasing perturbation
    // magnitude. The first non-zero coefficient determines the sign of the
    // result. See golang/geo's predicates.go for the full derivation.
    let checks: [i32; 3] = [b_cross_c.z.sign(), b_cross_c.y.sign(), b_cross_c.x.sign()]; // da.Z, da.Y, da.X
    for s in checks {
        if s != 0 {
            return Direction::from_sign(s);
        }
    }

    let det_sign = (&(&c.x * &a.y) - &(&c.y * &a.x)).sign(); // db.Z
    if det_sign != 0 {
        return Direction::from_sign(det_sign);
    }
    let det_sign = c.x.sign(); // db.Z * da.Y
    if det_sign != 0 {
        return Direction::from_sign(det_sign);
    }
    let det_sign = -c.y.sign(); // db.Z * da.X
    if det_sign != 0 {
        return Direction::from_sign(det_sign);
    }

    let det_sign = (&(&c.z * &a.x) - &(&c.x * &a.z)).sign(); // db.Y
    if det_sign != 0 {
        return Direction::from_sign(det_sign);
    }
    let det_sign = c.z.sign(); // db.Y * da.X
    if det_sign != 0 {
        return Direction::from_sign(det_sign);
    }

    // The following test is listed in the paper, but it is redundant because
    // the previous tests guarantee that C == (0, 0, 0).
    // (c.Y*a.Z - c.Z*a.Y).Sign() // db.X

    let det_sign = (&(&a.x * &b.y) - &(&a.y * &b.x)).sign(); // dc.Z
    if det_sign != 0 {
        return Direction::from_sign(det_sign);
    }
    let det_sign = -b.x.sign(); // dc.Z * da.Y
    if det_sign != 0 {
        return Direction::from_sign(det_sign);
    }
    let det_sign = b.y.sign(); // dc.Z * da.X
    if det_sign != 0 {
        return Direction::from_sign(det_sign);
    }
    let det_sign = a.x.sign(); // dc.Z * db.Y
    if det_sign != 0 {
        return Direction::from_sign(det_sign);
    }
    Direction::CounterClockwise // dc.Z * db.Y * da.X
}
