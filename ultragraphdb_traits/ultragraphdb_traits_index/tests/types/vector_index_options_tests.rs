/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_index::{
    KeyValue, SearchPathResult, VectorIndexOptions, accept_all, accept_none,
};

#[test]
fn test_vector_index_options() {
    let d = VectorIndexOptions::<f32>::default();
    assert_eq!(d.ef_override(), 0);
    assert_eq!(d.distance_threshold(), None);
    assert!((d.filter())(&[1.0], &[2.0], 7));
    let o = VectorIndexOptions::<f32>::new(10, Some(0.5), accept_none::<f32>);
    assert_eq!((o.ef_override(), o.distance_threshold()), (10, Some(0.5)));
    assert!(!(o.filter())(&[1.0], &[2.0], 7));
    assert!(accept_all::<f64>(&[], &[], 0));
}

#[test]
fn test_search_path_result_and_key_value() {
    let mut r = SearchPathResult::new();
    assert!(r.neighbors().is_empty() && r.path().is_empty() && r.metrics().is_empty());
    r.neighbors_mut().push(1);
    r.path_mut().push(2);
    r.metrics_mut().insert("visited".into(), 3);
    assert_eq!(
        (r.neighbors(), r.path(), r.metrics()["visited"]),
        (&[1][..], &[2][..], 3)
    );
    let kv = KeyValue::new(5, "attr", vec![1, 2]);
    assert_eq!(
        (kv.entity(), kv.attr(), kv.value()),
        (5, "attr", &[1u8, 2][..])
    );
}
