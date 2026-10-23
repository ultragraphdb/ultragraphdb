/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, r1/interval.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::gomath;

/// A closed interval on ℝ. Zero-length intervals (where lo == hi) represent single points.
/// If lo > hi then the interval is empty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct R1Interval {
    pub(crate) lo: f64,
    pub(crate) hi: f64,
}

impl R1Interval {
    /// Returns the interval [lo, hi].
    pub const fn new(lo: f64, hi: f64) -> Self {
        R1Interval { lo, hi }
    }

    /// Returns an empty interval.
    pub const fn empty() -> Self {
        R1Interval { lo: 1.0, hi: 0.0 }
    }

    /// Returns an interval representing a single point.
    pub const fn from_point(p: f64) -> Self {
        R1Interval { lo: p, hi: p }
    }

    /// The low endpoint.
    pub fn lo(&self) -> f64 {
        self.lo
    }

    /// The high endpoint.
    pub fn hi(&self) -> f64 {
        self.hi
    }

    /// Reports whether the interval is empty.
    pub fn is_empty(&self) -> bool {
        self.lo > self.hi
    }

    /// Returns true iff the interval contains the same points as oi.
    pub fn equal(&self, oi: R1Interval) -> bool {
        *self == oi || self.is_empty() && oi.is_empty()
    }

    /// Returns the midpoint of the interval. It is undefined for empty intervals.
    pub fn center(&self) -> f64 {
        0.5 * (self.lo + self.hi)
    }

    /// Returns the length of the interval. The length of an empty interval is negative.
    pub fn length(&self) -> f64 {
        self.hi - self.lo
    }

    /// Returns true iff the interval contains p.
    pub fn contains(&self, p: f64) -> bool {
        self.lo <= p && p <= self.hi
    }

    /// Returns true iff the interval contains oi.
    pub fn contains_interval(&self, oi: R1Interval) -> bool {
        if oi.is_empty() {
            return true;
        }
        self.lo <= oi.lo && oi.hi <= self.hi
    }

    /// Returns true iff the interval strictly contains p.
    pub fn interior_contains(&self, p: f64) -> bool {
        self.lo < p && p < self.hi
    }

    /// Returns true iff the interval contains any points in common with oi.
    pub fn intersects(&self, oi: R1Interval) -> bool {
        if self.lo <= oi.lo {
            return oi.lo <= self.hi && oi.lo <= oi.hi; // oi.lo ∈ i and oi is not empty
        }
        self.lo <= oi.hi && self.lo <= self.hi // i.lo ∈ oi and i is not empty
    }

    /// Returns the interval containing all points common to i and j.
    pub fn intersection(&self, j: R1Interval) -> R1Interval {
        // Empty intervals do not need to be special-cased.
        R1Interval {
            lo: gomath::max(self.lo, j.lo),
            hi: gomath::min(self.hi, j.hi),
        }
    }

    /// Returns the interval expanded so that it contains the given point.
    pub fn add_point(&self, p: f64) -> R1Interval {
        if self.is_empty() {
            return R1Interval { lo: p, hi: p };
        }
        if p < self.lo {
            return R1Interval { lo: p, hi: self.hi };
        }
        if p > self.hi {
            return R1Interval { lo: self.lo, hi: p };
        }
        *self
    }

    /// Returns the closest point in the interval to the given point p.
    /// The interval must be non-empty.
    pub fn clamp_point(&self, p: f64) -> f64 {
        gomath::max(self.lo, gomath::min(self.hi, p))
    }

    /// Returns an interval that has been expanded on each side by margin. If margin is
    /// negative, then the function shrinks the interval on each side by margin instead. The
    /// resulting interval may be empty. Any expansion of an empty interval remains empty.
    pub fn expanded(&self, margin: f64) -> R1Interval {
        if self.is_empty() {
            return *self;
        }
        R1Interval {
            lo: self.lo - margin,
            hi: self.hi + margin,
        }
    }

    /// Returns the smallest interval that contains this interval and the given interval.
    pub fn union(&self, other: R1Interval) -> R1Interval {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return *self;
        }
        R1Interval {
            lo: gomath::min(self.lo, other.lo),
            hi: gomath::max(self.hi, other.hi),
        }
    }
}
