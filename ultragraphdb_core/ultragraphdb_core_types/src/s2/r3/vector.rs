/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, r3/vector.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::gomath;
use crate::s2::s1::Angle;

/// A point in ℝ³.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct R3Vector {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) z: f64,
}

/// The 3 axes of ℝ³.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    X,
    Y,
    Z,
}

impl R3Vector {
    /// Returns the vector (x, y, z).
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        R3Vector { x, y, z }
    }

    /// The x coordinate.
    pub fn x(&self) -> f64 {
        self.x
    }

    /// The y coordinate.
    pub fn y(&self) -> f64 {
        self.y
    }

    /// The z coordinate.
    pub fn z(&self) -> f64 {
        self.z
    }

    /// Returns the vector's norm.
    pub fn norm(&self) -> f64 {
        self.dot(*self).sqrt()
    }

    /// Returns the square of the norm.
    pub fn norm2(&self) -> f64 {
        self.dot(*self)
    }

    /// Returns a unit vector in the same direction as v.
    pub fn normalize(&self) -> R3Vector {
        let n2 = self.norm2();
        if n2 == 0.0 {
            return R3Vector::new(0.0, 0.0, 0.0);
        }
        self.mul(1.0 / n2.sqrt())
    }

    /// Returns whether this vector is of approximately unit length.
    pub fn is_unit(&self) -> bool {
        const EPSILON: f64 = 5e-14;
        (self.norm2() - 1.0).abs() <= EPSILON
    }

    /// Returns the vector with nonnegative components.
    pub fn abs(&self) -> R3Vector {
        R3Vector::new(self.x.abs(), self.y.abs(), self.z.abs())
    }

    /// Returns the standard vector sum of v and ov.
    pub fn add(&self, ov: R3Vector) -> R3Vector {
        R3Vector::new(self.x + ov.x, self.y + ov.y, self.z + ov.z)
    }

    /// Returns the standard vector difference of v and ov.
    pub fn sub(&self, ov: R3Vector) -> R3Vector {
        R3Vector::new(self.x - ov.x, self.y - ov.y, self.z - ov.z)
    }

    /// Returns the standard scalar product of v and m.
    pub fn mul(&self, m: f64) -> R3Vector {
        R3Vector::new(m * self.x, m * self.y, m * self.z)
    }

    /// Returns the standard dot product of v and ov.
    pub fn dot(&self, ov: R3Vector) -> f64 {
        self.x * ov.x + self.y * ov.y + self.z * ov.z
    }

    /// Returns the standard cross product of v and ov.
    pub fn cross(&self, ov: R3Vector) -> R3Vector {
        R3Vector::new(
            self.y * ov.z - self.z * ov.y,
            self.z * ov.x - self.x * ov.z,
            self.x * ov.y - self.y * ov.x,
        )
    }

    /// Returns the angle between v and ov.
    pub fn angle(&self, ov: R3Vector) -> Angle {
        Angle::from_radians(gomath::atan2(self.cross(ov).norm(), self.dot(ov)))
    }

    /// Returns a unit vector that is orthogonal to v. Ortho(-v) = -Ortho(v) for all v.
    pub fn ortho(&self) -> R3Vector {
        let mut ov = R3Vector::default();
        match self.largest_component() {
            Axis::X => ov.z = 1.0,
            Axis::Y => ov.x = 1.0,
            Axis::Z => ov.y = 1.0,
        }
        self.cross(ov).normalize()
    }

    /// Returns the axis that represents the largest component in this vector.
    pub(crate) fn largest_component(&self) -> Axis {
        let t = self.abs();

        if t.x > t.y {
            if t.x > t.z {
                return Axis::X;
            }
            return Axis::Z;
        }
        if t.y > t.z {
            return Axis::Y;
        }
        Axis::Z
    }

    /// Compares v and ov lexicographically and returns -1 if v < ov, 0 if v == ov and
    /// +1 if v > ov, element by element with `<` and `>` as Go's `Cmp` does.
    pub fn compare(&self, ov: R3Vector) -> i32 {
        if self.x < ov.x {
            return -1;
        }
        if self.x > ov.x {
            return 1;
        }

        // First elements were the same, try the next.
        if self.y < ov.y {
            return -1;
        }
        if self.y > ov.y {
            return 1;
        }

        // Second elements were the same return the final compare.
        if self.z < ov.z {
            return -1;
        }
        if self.z > ov.z {
            return 1;
        }

        // Both are equal
        0
    }
}
