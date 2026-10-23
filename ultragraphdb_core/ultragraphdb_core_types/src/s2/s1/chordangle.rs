/*
 * Copyright 2015 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s1/chordangle.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::angle::Angle;
use crate::s2::gomath;

/// The square of the maximum length allowed in a ChordAngle.
const MAX_LENGTH2: f64 = 4.0;

/// The angle subtended by a chord (i.e., the straight line segment connecting two points on
/// the sphere), stored as the squared chord length. Only angles in [0, π] are representable.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct ChordAngle(pub(crate) f64);

impl ChordAngle {
    /// A chord angle smaller than the zero angle.
    pub const NEGATIVE: ChordAngle = ChordAngle(-1.0);
    /// A chord angle of 90 degrees (a "right angle").
    pub const RIGHT: ChordAngle = ChordAngle(2.0);
    /// A chord angle of 180 degrees (a "straight angle"), the maximum finite chord angle.
    pub const STRAIGHT: ChordAngle = ChordAngle(4.0);

    /// Returns a ChordAngle from the given Angle.
    pub fn from_angle(a: Angle) -> ChordAngle {
        if a.0 < 0.0 {
            return ChordAngle::NEGATIVE;
        }
        if a.is_inf() {
            return ChordAngle::inf();
        }
        let l = 2.0 * gomath::sin(0.5 * gomath::min(core::f64::consts::PI, a.radians()));
        ChordAngle(l * l)
    }

    /// Returns a ChordAngle from the squared chord length, clamped to at most 4.
    pub fn from_squared_length(length2: f64) -> ChordAngle {
        if length2 > MAX_LENGTH2 {
            return ChordAngle::STRAIGHT;
        }
        ChordAngle(length2)
    }

    /// The squared chord length.
    pub fn length2(&self) -> f64 {
        self.0
    }

    /// Returns a new ChordAngle that has been adjusted by the given error bound (which can be
    /// positive or negative).
    pub fn expanded(&self, e: f64) -> ChordAngle {
        // If the angle is special, don't change it. Otherwise clamp it to the valid range.
        if self.is_special() {
            return *self;
        }
        ChordAngle(gomath::max(0.0, gomath::min(MAX_LENGTH2, self.0 + e)))
    }

    /// Converts this ChordAngle to an Angle.
    pub fn angle(&self) -> Angle {
        if self.0 < 0.0 {
            return Angle(-1.0);
        }
        if self.is_infinity() {
            return Angle::inf();
        }
        Angle(2.0 * gomath::asin(0.5 * self.0.sqrt()))
    }

    /// Returns a chord angle larger than any finite chord angle.
    pub fn inf() -> ChordAngle {
        ChordAngle(f64::INFINITY)
    }

    /// Reports whether this ChordAngle is infinite.
    pub fn is_infinity(&self) -> bool {
        self.0 == f64::INFINITY
    }

    /// Reports whether this ChordAngle is one of the special cases.
    pub(crate) fn is_special(&self) -> bool {
        self.0 < 0.0 || self.is_infinity()
    }

    /// Adds the other ChordAngle to this one and returns the resulting value.
    /// This method assumes the ChordAngles are not special.
    pub fn add(&self, other: ChordAngle) -> ChordAngle {
        // Note that this method (and Sub) is much more efficient than converting
        // the ChordAngle to an Angle and adding those and converting back. It
        // requires only one square root plus a few additions and multiplications.

        // Optimization for the common case where b is an error tolerance
        // parameter that happens to be set to zero.
        if other.0 == 0.0 {
            return *self;
        }

        // Clamp the angle sum to at most 180 degrees.
        if self.0 + other.0 >= MAX_LENGTH2 {
            return ChordAngle::STRAIGHT;
        }

        // Let a and b be the (non-squared) chord lengths, and let c = a+b.
        // Let A, B, and C be the corresponding half-angles (a = 2*sin(A), etc).
        // Then the formula below can be derived from c = 2 * sin(A+B) and the
        // relationships   sin(A+B) = sin(A)*cos(B) + sin(B)*cos(A)
        //                 cos(X) = sqrt(1 - sin^2(X))
        let x = self.0 * (1.0 - 0.25 * other.0);
        let y = other.0 * (1.0 - 0.25 * self.0);
        ChordAngle(gomath::min(MAX_LENGTH2, x + y + 2.0 * (x * y).sqrt()))
    }

    /// Subtracts the other ChordAngle from this one and returns the resulting value.
    /// This method assumes the ChordAngles are not special.
    pub fn sub(&self, other: ChordAngle) -> ChordAngle {
        if other.0 == 0.0 {
            return *self;
        }
        if self.0 <= other.0 {
            return ChordAngle(0.0);
        }
        let x = self.0 * (1.0 - 0.25 * other.0);
        let y = other.0 * (1.0 - 0.25 * self.0);
        ChordAngle(gomath::max(0.0, x + y - 2.0 * (x * y).sqrt()))
    }

    /// Returns the sine of this chord angle.
    pub fn sin(&self) -> f64 {
        self.sin2().sqrt()
    }

    /// Returns the square of the sine of this chord angle.
    pub fn sin2(&self) -> f64 {
        // Let a be the (non-squared) chord length, and let A be the corresponding
        // half-angle (a = 2*sin(A)). The formula below can be derived from:
        //   sin(2*A) = 2 * sin(A) * cos(A)
        //   cos^2(A) = 1 - sin^2(A)
        // This is much faster than converting to an angle and computing its sine.
        self.0 * (1.0 - 0.25 * self.0)
    }
}
