/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s1/interval.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::gomath;
use core::f64::consts::PI;

/// The constant `2 * math.Pi` (exact doubling, so the float64 product equals Go's constant).
const TWO_PI: f64 = 2.0 * PI;
const MACHINE_EPSILON: f64 = f64::EPSILON;

/// A closed interval on a unit circle (also known as a 1-dimensional sphere). It is capable of
/// representing the empty interval (containing no points), the full interval (containing all
/// points), and zero-length intervals (containing a single point).
///
/// Points are represented by the angle they make with the positive x-axis in the range
/// [-π, π]. An interval is represented by its lower and upper bounds (both inclusive, since
/// the interval is closed). The lower bound may be greater than the upper bound, in which case
/// the interval is "inverted" (i.e. it passes through the point (-1, 0)).
///
/// The point (-1, 0) has two valid representations, π and -π. The normalized representation
/// of this point is π, so that endpoints of normal intervals are in the range (-π, π]. We
/// normalize the latter to the former in `from_endpoints`. However, we take advantage of the
/// point -π to construct two special intervals: the full interval is [-π, π] and the empty
/// interval is [π, -π].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct S1Interval {
    pub(crate) lo: f64,
    pub(crate) hi: f64,
}

impl S1Interval {
    /// Returns the interval [lo, hi] exactly as given, as Go's struct literal.
    pub const fn new(lo: f64, hi: f64) -> S1Interval {
        S1Interval { lo, hi }
    }

    /// Constructs a new interval from endpoints. Both arguments must be in the range [-π,π].
    /// This function allows inverted intervals to be created.
    pub fn from_endpoints(lo: f64, hi: f64) -> S1Interval {
        let mut i = S1Interval { lo, hi };
        if lo == -PI && hi != PI {
            i.lo = PI;
        }
        if hi == -PI && lo != PI {
            i.hi = PI;
        }
        i
    }

    /// Returns an empty interval.
    pub const fn empty() -> S1Interval {
        S1Interval { lo: PI, hi: -PI }
    }

    /// Returns a full interval.
    pub const fn full() -> S1Interval {
        S1Interval { lo: -PI, hi: PI }
    }

    /// The low endpoint.
    pub fn lo(&self) -> f64 {
        self.lo
    }

    /// The high endpoint.
    pub fn hi(&self) -> f64 {
        self.hi
    }

    /// Reports whether the interval is valid.
    pub fn is_valid(&self) -> bool {
        self.lo.abs() <= PI
            && self.hi.abs() <= PI
            && !(self.lo == -PI && self.hi != PI)
            && !(self.hi == -PI && self.lo != PI)
    }

    /// Reports whether the interval is full.
    pub fn is_full(&self) -> bool {
        self.lo == -PI && self.hi == PI
    }

    /// Reports whether the interval is empty.
    pub fn is_empty(&self) -> bool {
        self.lo == PI && self.hi == -PI
    }

    /// Reports whether the interval is inverted; that is, whether lo > hi.
    pub fn is_inverted(&self) -> bool {
        self.lo > self.hi
    }

    /// Returns the midpoint of the interval. It is undefined for full and empty intervals.
    pub fn center(&self) -> f64 {
        let c = 0.5 * (self.lo + self.hi);
        if !self.is_inverted() {
            return c;
        }
        if c <= 0.0 {
            return c + PI;
        }
        c - PI
    }

    /// Returns the length of the interval. The length of an empty interval is negative.
    pub fn length(&self) -> f64 {
        let mut l = self.hi - self.lo;
        if l >= 0.0 {
            return l;
        }
        l += TWO_PI;
        if l > 0.0 {
            return l;
        }
        -1.0
    }

    /// Returns true iff the interval contains p. Assumes p ∈ (-π, π].
    fn fast_contains(&self, p: f64) -> bool {
        if self.is_inverted() {
            return (p >= self.lo || p <= self.hi) && !self.is_empty();
        }
        p >= self.lo && p <= self.hi
    }

    /// Returns true iff the interval contains p. Assumes p ∈ [-π, π].
    pub fn contains(&self, p: f64) -> bool {
        let p = if p == -PI { PI } else { p };
        self.fast_contains(p)
    }

    /// Returns true iff the interval contains oi.
    pub fn contains_interval(&self, oi: S1Interval) -> bool {
        if self.is_inverted() {
            if oi.is_inverted() {
                return oi.lo >= self.lo && oi.hi <= self.hi;
            }
            return (oi.lo >= self.lo || oi.hi <= self.hi) && !self.is_empty();
        }
        if oi.is_inverted() {
            return self.is_full() || oi.is_empty();
        }
        oi.lo >= self.lo && oi.hi <= self.hi
    }

    /// Returns true iff the interval contains any points in common with oi.
    pub fn intersects(&self, oi: S1Interval) -> bool {
        if self.is_empty() || oi.is_empty() {
            return false;
        }
        if self.is_inverted() {
            return oi.is_inverted() || oi.lo <= self.hi || oi.hi >= self.lo;
        }
        if oi.is_inverted() {
            return oi.lo <= self.hi || oi.hi >= self.lo;
        }
        oi.lo <= self.hi && oi.hi >= self.lo
    }

    /// Returns the smallest interval that contains both the interval and oi.
    pub fn union(&self, oi: S1Interval) -> S1Interval {
        if oi.is_empty() {
            return *self;
        }
        if self.fast_contains(oi.lo) {
            if self.fast_contains(oi.hi) {
                // Either oi ⊂ i, or i ∪ oi is the full interval.
                if self.contains_interval(oi) {
                    return *self;
                }
                return S1Interval::full();
            }
            return S1Interval {
                lo: self.lo,
                hi: oi.hi,
            };
        }
        if self.fast_contains(oi.hi) {
            return S1Interval {
                lo: oi.lo,
                hi: self.hi,
            };
        }

        // Neither endpoint of oi is in i. Either i ⊂ oi, or i and oi are disjoint.
        if self.is_empty() || oi.fast_contains(self.lo) {
            return oi;
        }

        // This is the only hard case where we need to find the closest pair of endpoints.
        if positive_distance(oi.hi, self.lo) < positive_distance(self.hi, oi.lo) {
            return S1Interval {
                lo: oi.lo,
                hi: self.hi,
            };
        }
        S1Interval {
            lo: self.lo,
            hi: oi.hi,
        }
    }

    /// Returns the smallest interval that contains the intersection of the interval and oi.
    pub fn intersection(&self, oi: S1Interval) -> S1Interval {
        if oi.is_empty() {
            return S1Interval::empty();
        }
        if self.fast_contains(oi.lo) {
            if self.fast_contains(oi.hi) {
                // Either oi ⊂ i, or i and oi intersect twice. Neither are empty.
                // In the first case we want to return i (which is shorter than oi).
                // In the second case one of them is inverted, and the smallest interval
                // that covers the two disjoint pieces is the shorter of i and oi.
                // We thus want to pick the shorter of i and oi in both cases.
                if oi.length() < self.length() {
                    return oi;
                }
                return *self;
            }
            return S1Interval {
                lo: oi.lo,
                hi: self.hi,
            };
        }
        if self.fast_contains(oi.hi) {
            return S1Interval {
                lo: self.lo,
                hi: oi.hi,
            };
        }

        // Neither endpoint of oi is in i. Either i ⊂ oi, or i and oi are disjoint.
        if oi.fast_contains(self.lo) {
            return *self;
        }
        S1Interval::empty()
    }

    /// Returns the interval expanded by the minimum amount necessary such that it contains
    /// the given point "p" (an angle in the range [-π, π]).
    pub fn add_point(&self, p: f64) -> S1Interval {
        if p.abs() > PI {
            return *self;
        }
        let p = if p == -PI { PI } else { p };
        if self.fast_contains(p) {
            return *self;
        }
        if self.is_empty() {
            return S1Interval { lo: p, hi: p };
        }
        if positive_distance(p, self.lo) < positive_distance(self.hi, p) {
            return S1Interval { lo: p, hi: self.hi };
        }
        S1Interval { lo: self.lo, hi: p }
    }

    /// Returns an interval that has been expanded on each side by margin. If margin is
    /// negative, then the function shrinks the interval on each side by margin instead. The
    /// resulting interval may be empty or full. Any expansion (positive or negative) of a full
    /// interval remains full, and any expansion of an empty interval remains empty.
    pub fn expanded(&self, margin: f64) -> S1Interval {
        if margin >= 0.0 {
            if self.is_empty() {
                return *self;
            }
            // Check whether this interval will be full after expansion, allowing
            // for a rounding error when computing each endpoint.
            if self.length() + 2.0 * margin + 2.0 * MACHINE_EPSILON >= TWO_PI {
                return S1Interval::full();
            }
        } else {
            if self.is_full() {
                return *self;
            }
            // Check whether this interval will be empty after expansion, allowing
            // for a rounding error when computing each endpoint.
            if self.length() + 2.0 * margin - 2.0 * MACHINE_EPSILON <= 0.0 {
                return S1Interval::empty();
            }
        }
        let mut result = S1Interval::from_endpoints(
            gomath::remainder(self.lo - margin, TWO_PI),
            gomath::remainder(self.hi + margin, TWO_PI),
        );
        if result.lo <= -PI {
            result.lo = PI;
        }
        result
    }
}

/// Returns the distance from a to b in the range [0, 2π). This is equivalent to
///
/// ```text
/// result := math.Remainder(b - a, 2 * math.Pi)
/// if result < 0 {
///     result += 2 * math.Pi
/// }
/// ```
///
/// except that it is more numerically stable (it does not lose precision for very small
/// positive distances).
fn positive_distance(a: f64, b: f64) -> f64 {
    let d = b - a;
    if d >= 0.0 {
        return d;
    }
    // We want to ensure that if b == Pi and a == (-Pi + eps),
    // the return result is approximately 2*Pi and not zero.
    (b + PI) - (a - PI)
}
