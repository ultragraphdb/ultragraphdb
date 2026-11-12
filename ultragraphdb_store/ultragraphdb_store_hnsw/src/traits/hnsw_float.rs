/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/kernels.go and kernels_generic.go (dotF32, dotF64, euclideanSqF32,
 * euclideanSqF64, euclideanF32, euclideanF64, cosineSimF32, cosineSimF64) and tok/index/helper.go
 * (BytesAsFloatArray's element decoding). Changed for ultragraphdb: translated to Rust; one generic body per kernel
 * over the two widths, with Go's accumulators, unroll and summation order, unfused; a kernel reads the common
 * length of its two slices, where Go's reslice reads past a short subslice's length.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::ops::{Add, AddAssign, Div, Mul, Sub};

use ultragraphdb_traits_index::Float;

/// A vector element the index computes with, `f32` or `f64`: Go's `c.Float` with the distance kernels of
/// `kernels_generic.go` (the build without `GOEXPERIMENT=simd`, which Dgraph ships).
///
/// The kernels compute as Go does on amd64: four accumulators for dot product and squared euclidean distance, six
/// for cosine similarity, summed in Go's order, each multiply and add rounded on its own. Rust never fuses a
/// multiply-add, so the results equal Go's bit for bit.
pub trait HnswFloat:
    Float
    + Default
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + AddAssign
{
    /// The width in bits, the index's `floatBits`.
    const BITS: usize;

    /// The value as a float64: Go's `float64(v)`.
    fn to_f64(self) -> f64;

    /// The element encoded little-endian in `bytes`; `None` unless they hold exactly `BITS / 8` bytes.
    fn from_le(bytes: &[u8]) -> Option<Self>;

    /// Positive infinity.
    fn infinity() -> Self {
        Self::from_f64(f64::INFINITY)
    }

    /// Negative infinity.
    fn neg_infinity() -> Self {
        Self::from_f64(f64::NEG_INFINITY)
    }

    /// `sum(a[i]*b[i])`: `dotF32` and `dotF64`.
    fn dot(a: &[Self], b: &[Self]) -> Self {
        let (a, b) = common(a, b);
        let zero = Self::default();
        let (mut s0, mut s1, mut s2, mut s3) = (zero, zero, zero, zero);
        let ((a4, a_rest), (b4, b_rest)) = (a.as_chunks::<4>(), b.as_chunks::<4>());
        for (&[a0, a1, a2, a3], &[b0, b1, b2, b3]) in a4.iter().zip(b4) {
            s0 += a0 * b0;
            s1 += a1 * b1;
            s2 += a2 * b2;
            s3 += a3 * b3;
        }
        for (&x, &y) in a_rest.iter().zip(b_rest) {
            s0 += x * y;
        }
        (s0 + s1) + (s2 + s3)
    }

    /// `sum((a[i]-b[i])^2)`, not square-rooted: `euclideanSqF32` and `euclideanSqF64`.
    fn euclidean_sq(a: &[Self], b: &[Self]) -> Self {
        let (a, b) = common(a, b);
        let zero = Self::default();
        let (mut s0, mut s1, mut s2, mut s3) = (zero, zero, zero, zero);
        let ((a4, a_rest), (b4, b_rest)) = (a.as_chunks::<4>(), b.as_chunks::<4>());
        for (&[a0, a1, a2, a3], &[b0, b1, b2, b3]) in a4.iter().zip(b4) {
            let d0 = a0 - b0;
            let d1 = a1 - b1;
            let d2 = a2 - b2;
            let d3 = a3 - b3;
            s0 += d0 * d0;
            s1 += d1 * d1;
            s2 += d2 * d2;
            s3 += d3 * d3;
        }
        for (&x, &y) in a_rest.iter().zip(b_rest) {
            let d = x - y;
            s0 += d * d;
        }
        (s0 + s1) + (s2 + s3)
    }

    /// The square root of [`HnswFloat::euclidean_sq`], taken in float64: `euclideanF32` and `euclideanF64`.
    fn euclidean(a: &[Self], b: &[Self]) -> Self {
        Self::from_f64(Self::euclidean_sq(a, b).to_f64().sqrt())
    }

    /// `dot(a,b) / sqrt(dot(a,a)*dot(b,b))`, the root taken in float64; NaN for empty or zero vectors:
    /// `cosineSimF32` and `cosineSimF64`.
    fn cosine_sim(a: &[Self], b: &[Self]) -> Self {
        let (a, b) = common(a, b);
        let zero = Self::default();
        let (mut d0, mut d1, mut x0, mut x1, mut y0, mut y1) = (zero, zero, zero, zero, zero, zero);
        let ((a2, a_rest), (b2, b_rest)) = (a.as_chunks::<2>(), b.as_chunks::<2>());
        for (&[av0, av1], &[bv0, bv1]) in a2.iter().zip(b2) {
            d0 += av0 * bv0;
            d1 += av1 * bv1;
            x0 += av0 * av0;
            x1 += av1 * av1;
            y0 += bv0 * bv0;
            y1 += bv1 * bv1;
        }
        for (&x, &y) in a_rest.iter().zip(b_rest) {
            d0 += x * y;
            x0 += x * x;
            y0 += y * y;
        }
        let (dot, na, nb) = (d0 + d1, x0 + x1, y0 + y1);
        dot / Self::from_f64((na.to_f64() * nb.to_f64()).sqrt())
    }
}

/// The first `min(a.len(), b.len())` elements of each, which a kernel reads: equal lengths, so the two split into
/// chunks and remainders alike.
fn common<'v, T>(a: &'v [T], b: &'v [T]) -> (&'v [T], &'v [T]) {
    let n = a.len().min(b.len());
    let head = |v: &'v [T]| v.split_at_checked(n).map_or(v, |(head, _)| head);
    (head(a), head(b))
}

impl HnswFloat for f32 {
    const BITS: usize = 32;

    fn to_f64(self) -> f64 {
        f64::from(self)
    }

    fn from_le(bytes: &[u8]) -> Option<Self> {
        bytes.try_into().ok().map(f32::from_le_bytes)
    }
}

impl HnswFloat for f64 {
    const BITS: usize = 64;

    fn to_f64(self) -> f64 {
        self
    }

    fn from_le(bytes: &[u8]) -> Option<Self> {
        bytes.try_into().ok().map(f64::from_le_bytes)
    }
}
