/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s1/angle.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Go's constant `math.Pi / 180`, the size of a degree in radians.
pub(crate) const DEGREE: f64 = f64::from_bits(0x3f91df46a2529d39);

/// A 1D angle. The internal representation is a double precision value in radians.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Angle(pub(crate) f64);

impl Angle {
    /// Returns the angle of the given number of radians.
    pub const fn from_radians(radians: f64) -> Angle {
        Angle(radians)
    }

    /// Returns the angle of the given number of degrees, as Go's `s1.Angle(d) * s1.Degree`.
    pub fn from_degrees(degrees: f64) -> Angle {
        Angle(degrees * DEGREE)
    }

    /// Returns the angle in radians.
    pub fn radians(&self) -> f64 {
        self.0
    }

    /// Returns the angle in degrees.
    pub fn degrees(&self) -> f64 {
        self.0 / DEGREE
    }

    /// Returns the infinite angle.
    pub fn inf() -> Angle {
        Angle(f64::INFINITY)
    }

    /// Reports whether this angle is infinite.
    pub(crate) fn is_inf(&self) -> bool {
        self.0.is_infinite()
    }

    /// Returns the absolute value of the angle.
    pub fn abs(&self) -> Angle {
        Angle(self.0.abs())
    }
}
