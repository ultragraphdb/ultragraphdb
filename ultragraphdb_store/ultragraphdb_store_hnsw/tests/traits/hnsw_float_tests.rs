/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/kernels_test.go (TestKernelsFloat32, TestKernelsFloat64,
 * TestKernelsSelfDistance, TestKernelsDegenerate, TestKernelsMismatchedLength, TestKernelsNoAllocs; the file's
 * TestDistanceScoreLengthMismatch is in distance_tests.rs). Changed for ultragraphdb: translated to Rust; the random
 * vectors from the simulator's generator; added: the kernels' results against Go's, bit for bit, from the golden
 * vectors.
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_store_hnsw::HnswFloat;
use ultragraphdb_store_sim::SimRng;
use ultragraphdb_tool_bdd::{load_golden, testdata_dir};

/// A kernel of two vectors of `F`.
type Kernel<F> = fn(&[F], &[F]) -> F;

/// kernels_test.go's relative tolerance against a float64 reference.
const REL_TOL: f64 = 1e-5;
const KERNEL_DIMS: [usize; 22] = [
    0, 1, 2, 3, 4, 5, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 128, 384, 768, 1000, 1536,
];

fn rand_vec32(n: usize, seed: u64) -> Vec<f32> {
    let mut r = SimRng::new(seed);
    (0..n)
        .map(|_| ((r.next_u64() >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0)
        .collect()
}

fn rand_vec64(n: usize, seed: u64) -> Vec<f64> {
    let mut r = SimRng::new(seed);
    (0..n)
        .map(|_| ((r.next_u64() >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0)
        .collect()
}

fn ref_dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn ref_euclidean_sq(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

fn ref_cosine(a: &[f64], b: &[f64]) -> f64 {
    ref_dot(a, b) / (ref_dot(a, a) * ref_dot(b, b)).sqrt()
}

fn widen(v: &[f32]) -> Vec<f64> {
    v.iter().map(|&f| f64::from(f)).collect()
}

fn require_close(got: f64, want: f64, what: &str) {
    if want.abs() < 1e-6 {
        assert!((got - want).abs() <= 1e-6, "{what}: {got} vs {want}");
    } else {
        assert!(
            ((got - want) / want).abs() <= REL_TOL,
            "{what}: {got} vs {want}"
        );
    }
}

#[test]
fn test_kernels_float32() {
    for n in KERNEL_DIMS {
        let (a, b) = (rand_vec32(n, 1), rand_vec32(n, 2));
        let (a64, b64) = (widen(&a), widen(&b));
        require_close(f64::from(f32::dot(&a, &b)), ref_dot(&a64, &b64), "dot");
        require_close(
            f64::from(f32::euclidean_sq(&a, &b)),
            ref_euclidean_sq(&a64, &b64),
            "euclideanSq",
        );
        require_close(
            f64::from(f32::euclidean(&a, &b)),
            ref_euclidean_sq(&a64, &b64).sqrt(),
            "euclidean",
        );
        if n > 0 {
            require_close(
                f64::from(f32::cosine_sim(&a, &b)),
                ref_cosine(&a64, &b64),
                "cosine",
            );
        }
    }
}

#[test]
fn test_kernels_float64() {
    for n in KERNEL_DIMS {
        let (a, b) = (rand_vec64(n, 3), rand_vec64(n, 4));
        require_close(f64::dot(&a, &b), ref_dot(&a, &b), "dot");
        require_close(
            f64::euclidean_sq(&a, &b),
            ref_euclidean_sq(&a, &b),
            "euclideanSq",
        );
        require_close(
            f64::euclidean(&a, &b),
            ref_euclidean_sq(&a, &b).sqrt(),
            "euclidean",
        );
        if n > 0 {
            require_close(f64::cosine_sim(&a, &b), ref_cosine(&a, &b), "cosine");
        }
    }
}

#[test]
fn test_kernels_self_distance() {
    for n in [1, 8, 17, 768] {
        let a = rand_vec32(n, 5);
        assert_eq!(f32::euclidean_sq(&a, &a), 0.0);
        assert_eq!(f32::euclidean(&a, &a), 0.0);
        assert!((f64::from(f32::cosine_sim(&a, &a)) - 1.0).abs() <= REL_TOL);
        let d = rand_vec64(n, 6);
        assert_eq!(f64::euclidean_sq(&d, &d), 0.0);
        assert!((f64::cosine_sim(&d, &d) - 1.0).abs() <= REL_TOL);
    }
}

#[test]
fn test_kernels_degenerate() {
    let empty32: [f32; 0] = [];
    assert_eq!(f32::dot(&empty32, &empty32), 0.0);
    assert_eq!(f32::euclidean_sq(&empty32, &empty32), 0.0);
    assert_eq!(f32::euclidean(&empty32, &empty32), 0.0);
    assert!(
        f32::cosine_sim(&empty32, &empty32).is_nan(),
        "cosine of empty is 0/0"
    );
    let zeros = [0f32; 16];
    assert_eq!(f32::dot(&zeros, &zeros), 0.0);
    assert_eq!(f32::euclidean_sq(&zeros, &zeros), 0.0);
    assert!(
        f32::cosine_sim(&zeros, &zeros).is_nan(),
        "cosine of zero vector is 0/0"
    );
    let empty64: [f64; 0] = [];
    assert_eq!(f64::dot(&empty64, &empty64), 0.0);
    assert_eq!(f64::euclidean_sq(&empty64, &empty64), 0.0);
    assert!(
        f64::cosine_sim(&empty64, &empty64).is_nan(),
        "cosine of empty is 0/0"
    );
}

/// Deviation: Go's kernels reslice `b` to `len(a)`, panicking on a short `b` and reading past a short subslice's
/// length; the port's kernels read the two slices' common length and never panic. The distance functions refuse
/// different lengths before a kernel runs, in both.
#[test]
fn test_kernels_mismatched_length() {
    let a = rand_vec32(16, 7);
    let short = rand_vec32(8, 13);
    assert_eq!(f32::dot(&a, &short), f32::dot(&a[..8], &short));
    assert_eq!(
        f32::euclidean_sq(&a, &short),
        f32::euclidean_sq(&a[..8], &short)
    );
    assert_eq!(
        f32::cosine_sim(&a, &short).to_bits(),
        f32::cosine_sim(&a[..8], &short).to_bits()
    );
    let sub = &a[..8];
    assert_eq!(
        f32::dot(&a, sub),
        f32::dot(sub, sub),
        "a short subslice is read to its length only"
    );
}

/// Go's test counts allocations; Rust has no allocation counter without `unsafe`, which the workspace forbids. The
/// kernels' signature is the evidence: they borrow two slices and return one float, with no heap value to return.
#[test]
fn test_kernels_no_allocs() {
    let kernels32: [Kernel<f32>; 4] =
        [f32::dot, f32::euclidean_sq, f32::euclidean, f32::cosine_sim];
    let kernels64: [Kernel<f64>; 4] =
        [f64::dot, f64::euclidean_sq, f64::euclidean, f64::cosine_sim];
    let (a, b) = (rand_vec32(768, 9), rand_vec32(768, 10));
    let (c, d) = (rand_vec64(768, 11), rand_vec64(768, 12));
    for _ in 0..100 {
        kernels32
            .iter()
            .for_each(|k| assert!(!k(&a, &b).is_infinite()));
        kernels64
            .iter()
            .for_each(|k| assert!(!k(&c, &d).is_infinite()));
    }
}

#[derive(Debug, Deserialize)]
struct KernelCase {
    name: String,
    bits: usize,
    a: Vec<u64>,
    b: Vec<u64>,
    dot: u64,
    euclidean_sq: u64,
    euclidean: u64,
    cosine: u64,
}

#[test]
fn test_kernels_equal_gos_bit_for_bit() {
    let golden = load_golden::<KernelCase>(&testdata_dir().join("golden/hnsw_kernels.json"));
    assert_eq!(golden.cases().len(), 196);
    for c in golden.cases() {
        let got = if c.bits == 32 {
            let a: Vec<f32> = c.a.iter().map(|&x| f32::from_bits(x as u32)).collect();
            let b: Vec<f32> = c.b.iter().map(|&x| f32::from_bits(x as u32)).collect();
            [
                f32::dot(&a, &b),
                f32::euclidean_sq(&a, &b),
                f32::euclidean(&a, &b),
                f32::cosine_sim(&a, &b),
            ]
            .map(|f| u64::from(f.to_bits()))
        } else {
            let a: Vec<f64> = c.a.iter().map(|&x| f64::from_bits(x)).collect();
            let b: Vec<f64> = c.b.iter().map(|&x| f64::from_bits(x)).collect();
            [
                f64::dot(&a, &b),
                f64::euclidean_sq(&a, &b),
                f64::euclidean(&a, &b),
                f64::cosine_sim(&a, &b),
            ]
            .map(f64::to_bits)
        };
        assert_eq!(
            got,
            [c.dot, c.euclidean_sq, c.euclidean, c.cosine],
            "{} at {} bits",
            c.name,
            c.bits
        );
    }
}
