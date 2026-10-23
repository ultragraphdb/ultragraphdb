/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/constraints/constraints.go (Float). Changed for ultragraphdb: translated to Rust;
 * the integer constraints are Rust's own integer types and need no trait.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The element type of a vector, `float32` or `float64`: Go's `constraints.Float`.
pub trait Float: Copy + PartialOrd + std::fmt::Debug + Send + Sync + 'static {
    /// Go's `T(f)` of a float32.
    fn from_f32(f: f32) -> Self;

    /// Go's `T(f)` of a float64.
    fn from_f64(f: f64) -> Self;
}

impl Float for f32 {
    fn from_f32(f: f32) -> Self {
        f
    }

    fn from_f64(f: f64) -> Self {
        f as f32
    }
}

impl Float for f64 {
    fn from_f32(f: f32) -> Self {
        f64::from(f)
    }

    fn from_f64(f: f64) -> Self {
        f
    }
}
