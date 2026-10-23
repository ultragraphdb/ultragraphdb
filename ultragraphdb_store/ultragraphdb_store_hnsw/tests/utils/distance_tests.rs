/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/kernels_test.go (TestDistanceScoreLengthMismatch). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_store_hnsw::{HnswError, cosine_similarity, dot_product, euclidean_distance};

/// A distance function of two vectors.
type Distance = fn(&[f32], &[f32]) -> Result<f32, HnswError>;

#[test]
fn test_distance_score_length_mismatch() {
    let a: Vec<f32> = (0..16).map(|i| i as f32 / 16.0).collect();
    let fns: [(&str, Distance); 3] = [
        ("dot", dot_product::<f32>),
        ("cosine", cosine_similarity::<f32>),
        ("euclidean", euclidean_distance::<f32>),
    ];
    for (name, f) in fns {
        let err = f(&a, &a[..8]).expect_err(name);
        assert!(err.to_string().contains("different lengths"), "{name}");
    }
}
