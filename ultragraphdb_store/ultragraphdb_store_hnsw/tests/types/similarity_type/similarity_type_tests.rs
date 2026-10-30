/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw_test.go (TestSimilarityTypeIsSimilarityMetric). Changed for
 * ultragraphdb: translated to Rust; added: the scores' order and the sorted insertion.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_store_hnsw::{PersistentHeapElement, SimilarityType};
use ultragraphdb_traits_index::Metric;

#[test]
fn test_similarity_type_is_similarity_metric() {
    for (index_type, expected) in [
        ("euclidean", false),
        ("cosine", true),
        ("dotproduct", true),
        ("unknown", false),
    ] {
        assert_eq!(
            SimilarityType::from_name(index_type).is_similarity_metric(),
            expected,
            "{index_type}"
        );
    }
    assert_eq!(
        SimilarityType::from_name("unknown").index_type(),
        "euclidean"
    );
    assert_eq!(SimilarityType::new(Metric::Cosine).metric(), Metric::Cosine);
}

#[test]
fn test_scores_rank_by_metric() {
    let e = SimilarityType::new(Metric::Euclidean);
    let c = SimilarityType::new(Metric::Cosine);
    assert!(e.is_better_score(0.1f32, 0.2) && !e.is_better_score(0.2f32, 0.1));
    assert!(c.is_better_score(0.2f32, 0.1) && !c.is_better_score(0.1f32, 0.1));
    // A tie is no better under either order, and a dot product ranks its negative scores highest first too.
    assert!(!e.is_better_score(0.1f32, 0.1));
    let d = SimilarityType::new(Metric::DotProduct);
    assert!(d.is_better_score(-1.0f64, -2.0) && !d.is_better_score(-2.0f64, -1.0));
    assert!(!d.is_better_score(3.0f64, 3.0));
    assert_eq!(e.distance_score(&[0.0f32, 3.0], &[4.0, 0.0]), Ok(5.0));
    assert_eq!(c.distance_score(&[1.0f64, 0.0], &[2.0, 0.0]), Ok(1.0));
    assert_eq!(
        SimilarityType::new(Metric::DotProduct).distance_score(&[2.0f64, 1.0], &[3.0, 4.0]),
        Ok(10.0)
    );
}

#[test]
fn test_insort_keeps_the_best_first_and_equal_scores_in_arrival_order() {
    let el = |v: f64, i: u64| PersistentHeapElement::new(v, i, false);
    let mut ascending = vec![el(0.1, 1), el(0.5, 2)];
    SimilarityType::new(Metric::Euclidean).insort_heap(&mut ascending, el(0.5, 3));
    SimilarityType::new(Metric::Euclidean).insort_heap(&mut ascending, el(0.0, 4));
    assert_eq!(
        ascending.iter().map(|e| e.index()).collect::<Vec<_>>(),
        vec![4, 1, 2, 3]
    );
    let mut descending = vec![el(0.9, 1), el(0.5, 2)];
    SimilarityType::new(Metric::Cosine).insort_heap(&mut descending, el(0.5, 3));
    SimilarityType::new(Metric::Cosine).insort_heap(&mut descending, el(1.0, 4));
    assert_eq!(
        descending.iter().map(|e| e.index()).collect::<Vec<_>>(),
        vec![4, 1, 2, 3]
    );
}
