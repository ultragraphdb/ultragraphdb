/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_hnsw::insert_layer;

/// The level thresholds are Go's `math.Pow(1.0/5, n)`: a fifth of the nodes reach each level above the one below.
#[test]
fn test_the_insert_layer_follows_gos_thresholds() {
    assert_eq!(insert_layer(1, 0.999), 0);
    assert_eq!(insert_layer(3, 0.0), 0);
    // Go's math.Pow(0.2, 2) is 0.04000000000000001, as 0.2 * 0.2 rounds.
    assert_eq!(insert_layer(3, 0.04), 0);
    assert_eq!(insert_layer(3, 0.0401), 1);
    assert_eq!(insert_layer(3, 0.199), 1);
    assert_eq!(insert_layer(3, 0.2), 2);
    assert_eq!(insert_layer(3, 0.9999), 2);
    // 0.2^4 = 0.0016 in Go's arithmetic.
    assert_eq!(insert_layer(5, 0.00159), 0);
    assert_eq!(insert_layer(5, 0.0016001), 1);
    assert_eq!(
        insert_layer(4, f64::NAN),
        0,
        "no threshold passes, Go's zero level"
    );
}
