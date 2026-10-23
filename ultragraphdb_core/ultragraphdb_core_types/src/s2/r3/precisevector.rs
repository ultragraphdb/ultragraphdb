/*
 * Copyright 2016 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, r3/precisevector.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::exact_float::ExactFloat;
use super::vector::R3Vector;

/// A point in ℝ³ using exact values. Like Go's, it supports only the operations that need no
/// division or square root.
#[derive(Clone, Debug)]
pub(crate) struct PreciseVector {
    pub(crate) x: ExactFloat,
    pub(crate) y: ExactFloat,
    pub(crate) z: ExactFloat,
}

impl PreciseVector {
    /// Creates a high precision vector from the given Vector.
    pub(crate) fn from_vector(v: R3Vector) -> PreciseVector {
        PreciseVector {
            x: ExactFloat::from_f64(v.x),
            y: ExactFloat::from_f64(v.y),
            z: ExactFloat::from_f64(v.z),
        }
    }

    /// Returns the standard dot product of v and ov.
    pub(crate) fn dot(&self, ov: &PreciseVector) -> ExactFloat {
        &(&self.x * &ov.x) + &(&(&self.y * &ov.y) + &(&self.z * &ov.z))
    }

    /// Returns the standard cross product of v and ov.
    pub(crate) fn cross(&self, ov: &PreciseVector) -> PreciseVector {
        PreciseVector {
            x: &(&self.y * &ov.z) - &(&self.z * &ov.y),
            y: &(&self.z * &ov.x) - &(&self.x * &ov.z),
            z: &(&self.x * &ov.y) - &(&self.y * &ov.x),
        }
    }
}
