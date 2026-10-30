/*
 * Copyright 2017 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/rect_bounder.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::gomath;
use crate::s2::latlng::LatLng;
use crate::s2::point::Point;
use crate::s2::r1::R1Interval;
use crate::s2::r3::R3Vector;
use crate::s2::rect::Rect;
use crate::s2::s1::{Angle, S1Interval};
use core::f64::consts::{FRAC_PI_2, PI};

const MACHINE_EPSILON: f64 = f64::EPSILON;

/// Used to compute a bounding rectangle that contains all edges defined by a vertex chain
/// (v0, v1, v2, ...). All vertices must be unit length. Note that the bounding rectangle of
/// an edge can be larger than the bounding rectangle of its endpoints, e.g. consider an edge
/// that passes through the north pole.
///
/// The bounds are calculated conservatively to account for numerical errors when points are
/// converted to LatLngs. More precisely, this function guarantees the following: Let L be a
/// closed edge chain (Loop) such that the interior of the loop does not contain either pole.
/// Now if P is any point such that L.ContainsPoint(P), then RectBound(L).ContainsPoint(LatLngFromPoint(P)).
#[derive(Clone, Copy, Debug)]
pub struct RectBounder {
    // The previous vertex in the chain.
    a: Point,
    // The previous vertex latitude longitude.
    a_ll: LatLng,
    bound: Rect,
}

impl Default for RectBounder {
    fn default() -> Self {
        RectBounder::new()
    }
}

impl RectBounder {
    /// Returns a new instance of a RectBounder.
    pub fn new() -> RectBounder {
        RectBounder {
            a: Point::default(),
            a_ll: LatLng::default(),
            bound: Rect::empty(),
        }
    }

    /// Adds the given point to the chain. The Point must be unit length.
    pub fn add_point(&mut self, b: Point) {
        let b_ll = LatLng::from_point(b);

        if self.bound.is_empty() {
            self.a = b;
            self.a_ll = b_ll;
            self.bound = self.bound.add_point(b_ll);
            return;
        }

        // First compute the cross product N = A x B robustly. This is the normal
        // to the great circle through A and B. We don't use RobustSign
        // since that method returns an arbitrary vector orthogonal to A if the two
        // vectors are proportional, and we want the zero vector in that case.
        let n = self.a.0.sub(b.0).cross(self.a.0.add(b.0)); // N = 2 * (A x B)

        // The relative error in N gets large as its norm gets very small (i.e.,
        // when the two points are nearly identical or antipodal). We handle this
        // by choosing a maximum allowable error, and if the error is greater than
        // this we fall back to a different technique. Since it turns out that
        // the other sources of error in converting the normal to a maximum
        // latitude add up to at most 1.16 * dblEpsilon, and it is desirable to
        // have the total error be a multiple of dblEpsilon, we have chosen to
        // limit the maximum error in the normal to be 3.84 * dblEpsilon.
        // It is possible to show that the error is less than this when
        //
        // n.Norm() >= 8 * sqrt(3) / (3.84 - 0.5 - sqrt(3)) * dblEpsilon
        //          = 1.91346e-15 (about 8.618 * dblEpsilon)
        let n_norm = n.norm();
        if n_norm < 1.91346e-15 {
            // A and B are either nearly identical or nearly antipodal (to within
            // 4.309 * dblEpsilon, or about 6 nanometers on the earth's surface).
            if self.a.0.dot(b.0) < 0.0 {
                // The two points are nearly antipodal. The easiest solution is to
                // assume that the edge between A and B could go in any direction
                // around the sphere.
                self.bound = Rect::full();
            } else {
                // The two points are nearly identical (to within 4.309 * dblEpsilon).
                // In this case we can just use the bounding rectangle of the points,
                // since after the expansion done by GetBound this Rect is
                // guaranteed to include the (lat,lng) values of all points along AB.
                self.bound = self
                    .bound
                    .union(Rect::from_lat_lng(self.a_ll).add_point(b_ll));
            }
            self.a = b;
            self.a_ll = b_ll;
            return;
        }

        // Compute the longitude range spanned by AB.
        let mut lng_ab = S1Interval::empty()
            .add_point(self.a_ll.lng.radians())
            .add_point(b_ll.lng.radians());
        if lng_ab.length() >= PI - 2.0 * MACHINE_EPSILON {
            // The points lie on nearly opposite lines of longitude to within the
            // maximum error of the calculation. The easiest solution is to assume
            // that AB could go on either side of the pole.
            lng_ab = S1Interval::full();
        }

        // Next we compute the latitude range spanned by the edge AB. We start
        // with the range spanning the two endpoints of the edge:
        let mut lat_ab =
            R1Interval::from_point(self.a_ll.lat.radians()).add_point(b_ll.lat.radians());

        // This is the desired range unless the edge AB crosses the plane
        // through N and the Z-axis (which is where the great circle through A
        // and B attains its minimum and maximum latitudes). To test whether AB
        // crosses this plane, we compute a vector M perpendicular to this
        // plane and then project A and B onto it.
        let m = n.cross(R3Vector::new(0.0, 0.0, 1.0));
        let m_a = m.dot(self.a.0);
        let m_b = m.dot(b.0);

        // We want to test the signs of "mA" and "mB", so we need to bound
        // the error in these calculations. It is possible to show that the
        // total error is bounded by
        //
        // (1 + sqrt(3)) * dblEpsilon * nNorm + 8 * sqrt(3) * (dblEpsilon**2)
        //   = 6.06638e-16 * nNorm + 6.83174e-31
        let m_error = 6.06638e-16 * n_norm + 6.83174e-31;
        if m_a * m_b < 0.0 || m_a.abs() <= m_error || m_b.abs() <= m_error {
            // Minimum/maximum latitude *may* occur in the edge interior.
            //
            // The maximum latitude is 90 degrees minus the latitude of N. We
            // compute this directly using atan2 in order to get maximum accuracy
            // near the poles.
            //
            // Our goal is compute a bound that contains the computed latitudes of
            // all S2Points P that pass the point-in-polygon containment test.
            // There are three sources of error we need to consider:
            // - the directional error in N (at most 3.84 * dblEpsilon)
            // - converting N to a maximum latitude
            // - computing the latitude of the test point P
            // The latter two sources of error are at most 0.955 * dblEpsilon
            // individually, but it is possible to show by a more complex analysis
            // that together they can add up to at most 1.16 * dblEpsilon, for a
            // total error of 5 * dblEpsilon.
            //
            // We add 3 * dblEpsilon to the bound here, and GetBound() will pad
            // the bound by another 2 * dblEpsilon.
            let max_lat = gomath::min(
                gomath::atan2((n.x * n.x + n.y * n.y).sqrt(), n.z.abs()) + 3.0 * MACHINE_EPSILON,
                FRAC_PI_2,
            );

            // In order to get tight bounds when the two points are close together,
            // we also bound the min/max latitude relative to the latitudes of the
            // endpoints A and B. First we compute the distance between A and B,
            // and then we compute the maximum change in latitude between any two
            // points along the great circle that are separated by this distance.
            // This gives us a latitude change "budget". Some of this budget must
            // be spent getting from A to B; the remainder bounds the round-trip
            // distance (in latitude) from A or B to the min or max latitude
            // attained along the edge AB.
            let lat_budget =
                2.0 * gomath::asin(0.5 * self.a.0.sub(b.0).norm() * gomath::sin(max_lat));
            let max_delta = 0.5 * (lat_budget - lat_ab.length()) + MACHINE_EPSILON;

            // Test whether AB passes through the point of maximum latitude or
            // minimum latitude. If the dot product(s) are small enough then the
            // result may be ambiguous.
            if m_a <= m_error && m_b >= -m_error {
                lat_ab.hi = gomath::min(max_lat, lat_ab.hi + max_delta);
            }
            if m_b <= m_error && m_a >= -m_error {
                lat_ab.lo = gomath::max(-max_lat, lat_ab.lo - max_delta);
            }
        }
        self.a = b;
        self.a_ll = b_ll;
        self.bound = self.bound.union(Rect::new(lat_ab, lng_ab));
    }

    /// Returns the bounding rectangle of the edge chain that connects the vertices defined so
    /// far. This bound satisfies the guarantee made above, i.e. if the edge chain defines a
    /// Loop, then the bound contains the LatLng coordinates of all Points contained by the
    /// loop.
    pub fn rect_bound(&self) -> Rect {
        self.bound
            .expanded(LatLng::new(Angle(2.0 * MACHINE_EPSILON), Angle(0.0)))
            .polar_closure()
    }
}

/// Expands a bounding Rect so that it is guaranteed to contain the bounds of any subregion
/// whose bounds are computed using ComputeRectBound. For example, consider a loop L that
/// defines a square. GetBound ensures that if a point P is contained by this square, then
/// LatLngFromPoint(P) is contained by the bound. But now consider a diamond shaped loop S
/// contained by L. It is possible that GetBound returns a *larger* bound for S than it does
/// for L, due to rounding errors. This method expands the bound for L so that it is
/// guaranteed to contain the bounds of any subregion S.
///
/// More precisely, if L is a loop that does not contain either pole, and S is a loop such
/// that L.Contains(S), then
///
/// ```text
/// ExpandForSubregions(RectBound(L)).Contains(RectBound(S)).
/// ```
pub fn expand_for_subregions(bound: Rect) -> Rect {
    // Empty bounds don't need expansion.
    if bound.is_empty() {
        return bound;
    }

    // First we need to check whether the bound B contains any nearly-antipodal
    // points (to within 4.309 * dblEpsilon). If so then we need to return
    // FullRect, since the subregion might have an edge between two
    // such points, and AddPoint returns Full for such edges. Note that
    // this can happen even if B is not Full for example, consider a loop
    // that defines a 10km strip straddling the equator extending from
    // longitudes -100 to +100 degrees.
    //
    // It is easy to check whether B contains any antipodal points, but checking
    // for nearly-antipodal points is trickier. Essentially we consider the
    // original bound B and its reflection through the origin B', and then test
    // whether the minimum distance between B and B' is less than 4.309 * dblEpsilon.

    // lngGap is a lower bound on the longitudinal distance between B and its
    // reflection B'. (2.5 * dblEpsilon is the maximum combined error of the
    // endpoint longitude calculations and the Length call.)
    let lng_gap = gomath::max(0.0, PI - bound.lng.length() - 2.5 * MACHINE_EPSILON);

    // minAbsLat is the minimum distance from B to the equator (if zero or
    // negative, then B straddles the equator).
    let min_abs_lat = gomath::max(bound.lat.lo, -bound.lat.hi);

    // latGapSouth and latGapNorth measure the minimum distance from B to the
    // south and north poles respectively.
    let lat_gap_south = FRAC_PI_2 + bound.lat.lo;
    let lat_gap_north = FRAC_PI_2 - bound.lat.hi;

    if min_abs_lat >= 0.0 {
        // The bound B does not straddle the equator. In this case the minimum
        // distance is between one endpoint of the latitude edge in B closest to
        // the equator and the other endpoint of that edge in B'. The latitude
        // distance between these two points is 2*minAbsLat, and the longitude
        // distance is lngGap. We could compute the distance exactly using the
        // Haversine formula, but then we would need to bound the errors in that
        // calculation. Since we only need accuracy when the distance is very
        // small (close to 4.309 * dblEpsilon), we substitute the Euclidean
        // distance instead. This gives us a right triangle XYZ with two edges of
        // length x = 2*minAbsLat and y ~= lngGap. The desired distance is the
        // length of the third edge z, and we have
        //
        //         z  ~=  sqrt(x^2 + y^2)  >=  (x + y) / sqrt(2)
        //
        // Therefore the region may contain nearly antipodal points only if
        //
        //  2*minAbsLat + lngGap  <  sqrt(2) * 4.309 * dblEpsilon
        //                        ~= 1.354e-15
        //
        // Note that because the given bound B is conservative, minAbsLat and
        // lngGap are both lower bounds on their true values so we do not need
        // to make any adjustments for their errors.
        if 2.0 * min_abs_lat + lng_gap < 1.354e-15 {
            return Rect::full();
        }
    } else if lng_gap >= FRAC_PI_2 {
        // B spans at most Pi/2 in longitude. The minimum distance is always
        // between one corner of B and the diagonally opposite corner of B'. We
        // use the same distance approximation that we used above; in this case
        // we have an obtuse triangle XYZ with two edges of length x = latGapSouth
        // and y = latGapNorth, and angle Z >= Pi/2 between them. We then have
        //
        //         z  >=  sqrt(x^2 + y^2)  >=  (x + y) / sqrt(2)
        //
        // Unlike the case above, latGapSouth and latGapNorth are not lower bounds
        // (because of the extra addition operation, and because math.Pi/2 is not
        // exactly equal to Pi/2); they can exceed their true values by up to
        // 0.75 * dblEpsilon. Putting this all together, the region may contain
        // nearly antipodal points only if
        //
        //   latGapSouth + latGapNorth  <  (sqrt(2) * 4.309 + 1.5) * dblEpsilon
        //                              ~= 1.687e-15
        if lat_gap_south + lat_gap_north < 1.687e-15 {
            return Rect::full();
        }
    } else {
        // Otherwise we know that (1) the bound straddles the equator and (2) its
        // width in longitude is at least Pi/2. In this case the minimum
        // distance can occur either between a corner of B and the diagonally
        // opposite corner of B' (as in the case above), or between a corner of B
        // and the opposite longitudinal edge reflected in B'. It is sufficient
        // to only consider the corner-edge case, since this distance is also a
        // lower bound on the corner-corner distance when that case applies.

        // Consider the spherical triangle XYZ where X is a corner of B with
        // minimum absolute latitude, Y is the closest pole to X, and Z is the
        // point closest to X on the opposite longitudinal edge of B'. This is a
        // right triangle (Z = Pi/2), and from the spherical law of sines we have
        //
        //     sin(z) / sin(Z)  =  sin(y) / sin(Y)
        //     sin(maxLatGap) / 1  =  sin(dMin) / sin(lngGap)
        //     sin(dMin)  =  sin(maxLatGap) * sin(lngGap)
        //
        // where "maxLatGap" = max(latGapSouth, latGapNorth) and "dMin" is the
        // desired minimum distance. Now using the facts that sin(t) >= (2/Pi)*t
        // for 0 <= t <= Pi/2, that we only need an accurate approximation when
        // at least one of "maxLatGap" or lngGap is extremely small (in which
        // case sin(t) ~= t), and recalling that "maxLatGap" has an error of up
        // to 0.75 * dblEpsilon, we want to test whether
        //
        //   maxLatGap * lngGap  <  (4.309 + 0.75) * (Pi/2) * dblEpsilon
        //                       ~= 1.765e-15
        if gomath::max(lat_gap_south, lat_gap_north) * lng_gap < 1.765e-15 {
            return Rect::full();
        }
    }

    // Next we need to check whether the subregion might contain any edges that
    // span (math.Pi - 2 * dblEpsilon) radians or more in longitude, since AddPoint
    // sets the longitude bound to Full in that case. This corresponds to
    // testing whether (lngGap <= 0) in lngExpansion below.

    // Otherwise, the maximum latitude error in AddPoint is 4.8 * dblEpsilon.
    // In the worst case, the errors when computing the latitude bound for a
    // subregion could go in the opposite direction as the errors when computing
    // the bound for the original region, so we need to double this value.
    // (More analysis could probably reduce this constant factor.)
    //
    // We also need to add some extra margin in latitude to handle the case
    // where AddPoint is called with nearly antipodal points.
    let lat_expansion = 9.0 * MACHINE_EPSILON;
    let lng_expansion = if lng_gap <= 0.0 { PI } else { 0.0 };
    bound
        .expanded(LatLng::new(Angle(lat_expansion), Angle(lng_expansion)))
        .polar_closure()
}
