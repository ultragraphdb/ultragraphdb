/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_index::{COSINE, DOT_PROD, EUCLIDEAN, Metric};

#[test]
fn test_metrics_are_named_as_dgraphs_metric_option() {
    assert_eq!(
        (EUCLIDEAN, COSINE, DOT_PROD),
        ("euclidean", "cosine", "dotproduct")
    );
    for m in [Metric::Euclidean, Metric::Cosine, Metric::DotProduct] {
        assert_eq!(Metric::from_name(m.name()), Some(m));
    }
    assert_eq!(Metric::from_name("l1"), None);
}
