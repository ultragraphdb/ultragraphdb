/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/kernels_test.go (TestDistanceScoreLengthMismatch). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_store_hnsw::{
    HnswError, HnswErrorEnum, cosine_similarity, dot_product, euclidean_distance,
};

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
    // Go's names of the three functions in its error, as the Go 1.27 probe prints them.
    for ((name, f), func_name) in
        fns.into_iter()
            .zip(["dot product", "cosine distance", "euclidean distance"])
    {
        for (a, b) in [(&a[..], &a[..8]), (&a[..0], &a[..1])] {
            let err = f(a, b).expect_err(name);
            assert_eq!(err.0, HnswErrorEnum::DifferentLengths(func_name), "{name}");
            assert_eq!(
                err.to_string(),
                format!("can not compute {func_name} on vectors of different lengths")
            );
        }
    }
}

/// The three distances at both widths, by hand: (2,1)·(3,4) = 10; |(0,3)-(4,0)| = 5; cos((3,4),(4,3)) = 24/25;
/// (1,2)·(-3,1) = -1, a negative score; two empty vectors have dot product and distance 0 and cosine 0/0.
#[test]
fn test_the_distances_by_hand() {
    assert_eq!(dot_product(&[2.0f32, 1.0], &[3.0, 4.0]), Ok(10.0));
    assert_eq!(dot_product(&[1.0f64, 2.0], &[-3.0, 1.0]), Ok(-1.0));
    assert_eq!(euclidean_distance(&[0.0f32, 3.0], &[4.0, 0.0]), Ok(5.0));
    assert_eq!(euclidean_distance(&[0.0f64, 3.0], &[4.0, 0.0]), Ok(5.0));
    assert_eq!(
        cosine_similarity(&[3.0f64, 4.0], &[4.0, 3.0]),
        Ok(24.0 / 25.0)
    );
    assert_eq!(
        cosine_similarity(&[3.0f32, 4.0], &[4.0, 3.0]),
        Ok(24.0f32 / 25.0)
    );
    let empty: [f64; 0] = [];
    assert_eq!(dot_product(&empty, &empty), Ok(0.0));
    assert_eq!(euclidean_distance(&empty, &empty), Ok(0.0));
    assert!(cosine_similarity(&empty, &empty).is_ok_and(f64::is_nan));
    assert!(cosine_similarity(&[0.0f32, 0.0], &[1.0, 0.0]).is_ok_and(f32::is_nan));
}
