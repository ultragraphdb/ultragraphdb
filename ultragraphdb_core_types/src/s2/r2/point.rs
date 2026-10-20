/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, r2/rect.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// A point in ℝ².
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct R2Point {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl R2Point {
    /// Returns the point (x, y).
    pub const fn new(x: f64, y: f64) -> Self {
        R2Point { x, y }
    }

    /// The x coordinate.
    pub fn x(&self) -> f64 {
        self.x
    }

    /// The y coordinate.
    pub fn y(&self) -> f64 {
        self.y
    }

    /// Returns the sum of p and op.
    pub fn add(&self, op: R2Point) -> R2Point {
        R2Point {
            x: self.x + op.x,
            y: self.y + op.y,
        }
    }

    /// Returns the difference of p and op.
    pub fn sub(&self, op: R2Point) -> R2Point {
        R2Point {
            x: self.x - op.x,
            y: self.y - op.y,
        }
    }

    /// Returns the scalar product of p and m.
    pub fn mul(&self, m: f64) -> R2Point {
        R2Point {
            x: m * self.x,
            y: m * self.y,
        }
    }

    /// Returns a counterclockwise orthogonal point with the same norm.
    pub fn ortho(&self) -> R2Point {
        R2Point {
            x: -self.y,
            y: self.x,
        }
    }

    /// Returns the dot product between p and op.
    pub fn dot(&self, op: R2Point) -> f64 {
        self.x * op.x + self.y * op.y
    }

    /// Returns the cross product of p and op.
    pub fn cross(&self, op: R2Point) -> f64 {
        self.x * op.y - self.y * op.x
    }
}
