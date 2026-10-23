/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw_test.go (TestBasicSearchPersistentFlatStorage,
 * TestSearchReturnsCorrectOrderForAllMetrics) and ef_recall_test.go (TestHNSWSearchEfOverrideImprovesRecall,
 * TestHNSWDistanceThreshold_Euclidean, TestHNSWDistanceThreshold_Cosine). Changed for ultragraphdb: translated to
 * Rust; every insert builds its index, where Go's tests share one; the synthetic graphs are stored neighbour lists,
 * where Go's tests write the index's in-memory cache; added: the empty index, a result count of 0, and the path and
 * counters of a search.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_store_hnsw::{
    HnswErrorEnum, PersistentHnsw, QueryCache, TxnCache, encode_uint64_matrix, uint64_to_bytes,
};
use ultragraphdb_store_sim::SimRng;
use ultragraphdb_traits_index::{VectorIndexOptions, accept_all, accept_none};

use crate::utils::helpers::{
    InMemLocalCache, InMemTxn, MapTxn, MemoryCache, TsDbs, draw, float64_array_as_bytes, key, opts,
};

fn index(metric: &str, max_levels: &str) -> PersistentHnsw<f64> {
    let o = opts(&[
        ("maxLevels", max_levels),
        ("efConstruction", "16"),
        ("efSearch", "12"),
        ("metric", metric),
    ]);
    PersistentHnsw::new("0-a", &o, 64).unwrap()
}

#[test]
fn test_basic_search_persistent_flat_storage() {
    for metric in ["euclidean", "cosine", "dotproduct"] {
        let dbs = TsDbs::default();
        let mut rng = SimRng::new(4);
        for (start, commit, uid, vec) in [
            (0, 1, 5, [0.1, 0.1, 0.1]),
            (11, 15, 123, [0.824, 0.319, 0.111]),
            (20, 37, 1, [0.3, 0.5, 0.7]),
        ] {
            dbs.set_all(&key("0-a", uid), float64_array_as_bytes(&vec));
            let txn = InMemTxn::new(&dbs, start, commit);
            index(metric, "5")
                .insert(&TxnCache::new(&txn, start), uid, &vec, &mut || {
                    draw(&mut rng)
                })
                .unwrap();
        }
        for (read_ts, query, expected) in [
            (45, [0.3, 0.5, 0.7], vec![1]),
            (93, [0.824, 0.319, 0.111], vec![123]),
        ] {
            let lc = InMemLocalCache::new(&dbs, read_ts);
            let nns = index(metric, "5")
                .search(&QueryCache::new(&lc, read_ts), &query, 1, accept_all::<f64>)
                .unwrap();
            assert_eq!(nns, expected, "{metric} at {read_ts}");
        }
    }
}

#[test]
fn test_search_returns_correct_order_for_all_metrics() {
    for metric in ["euclidean", "cosine", "dotproduct"] {
        let dbs = TsDbs::default();
        let pred = format!("0-test{metric}");
        let o = opts(&[
            ("maxLevels", "1"),
            ("efConstruction", "16"),
            ("efSearch", "12"),
            ("metric", metric),
        ]);
        for (start, uid, vec) in [
            (0, 10, [1.0, 0.0, 0.0]),
            (1, 20, [0.9, 0.1, 0.0]),
            (2, 30, [0.1, 0.9, 0.0]),
        ] {
            dbs.set_all(&key(&pred, uid), float64_array_as_bytes(&vec));
            let txn = InMemTxn::new(&dbs, start, start + 1);
            PersistentHnsw::<f64>::new(&pred, &o, 64)
                .unwrap()
                .insert(&TxnCache::new(&txn, start), uid, &vec, &mut || 0.5)
                .unwrap();
        }
        let lc = InMemLocalCache::new(&dbs, 10);
        let results = PersistentHnsw::<f64>::new(&pred, &o, 64)
            .unwrap()
            .search(
                &QueryCache::new(&lc, 10),
                &[1.0, 0.0, 0.0],
                2,
                accept_all::<f64>,
            )
            .unwrap();
        assert!(!results.is_empty());
        assert_ne!(results[0], 30, "{metric}: the farthest vector first");
    }
}

/// The predicate of a synthetic graph, its vectors, the entry point and every node's stored neighbour lists.
fn synthetic(
    pred: &str,
    vectors: &[(u64, Vec<f64>)],
    edges: &[(u64, Vec<Vec<u64>>)],
) -> MemoryCache {
    let mut cache = MemoryCache::default();
    for (uid, v) in vectors {
        cache
            .data
            .insert(key(pred, *uid), float64_array_as_bytes(v));
    }
    cache
        .data
        .insert(key(&format!("{pred}{VEC_ENTRY}"), 1), uint64_to_bytes(1));
    for (uid, lists) in edges {
        cache.data.insert(
            key(&format!("{pred}{VEC_KEYWORD}"), *uid),
            encode_uint64_matrix(lists),
        );
    }
    cache
}

#[test]
fn test_hnsw_search_ef_override_improves_recall() {
    let pred = "0-ef_override_test_pred";
    let o = opts(&[
        ("maxLevels", "2"),
        ("efSearch", "1"),
        ("metric", "euclidean"),
    ]);
    let ph = PersistentHnsw::<f64>::new(pred, &o, 64).unwrap();
    assert_eq!(ph.pred(), pred);
    let cache = synthetic(
        pred,
        &[
            (1, vec![0.0, 0.0, 10.0, 0.0]),
            (100, vec![0.0, 0.0, 0.1, 0.0]),
            (200, vec![0.0, 0.0, 3.0, 0.0]),
            (201, vec![0.0, 0.0, 3.2, 0.0]),
        ],
        &[
            (1, vec![vec![], vec![200, 201]]),
            (200, vec![vec![1], vec![1]]),
            (201, vec![vec![1], vec![100]]),
            (100, vec![vec![201], vec![201]]),
        ],
    );
    let query = [0.0, 0.0, 0.12, 0.0];
    let narrow = ph.search_with_options(&cache, &query, 1, &VectorIndexOptions::default());
    assert_eq!(narrow, Ok(vec![200]));
    let wide = ph.search_with_options(
        &cache,
        &query,
        1,
        &VectorIndexOptions::new(4, None, accept_all::<f64>),
    );
    assert_eq!(wide, Ok(vec![100]));
}

#[test]
fn test_hnsw_distance_threshold_euclidean() {
    let pred = "0-thresh_pred_e";
    let o = opts(&[
        ("maxLevels", "1"),
        ("efSearch", "10"),
        ("metric", "euclidean"),
    ]);
    let ph = PersistentHnsw::<f64>::new(pred, &o, 64).unwrap();
    let cache = synthetic(
        pred,
        &[(1, vec![0.6, 0.0]), (2, vec![0.8, 0.0])],
        &[(1, vec![vec![1, 2]]), (2, vec![vec![1]])],
    );
    let q = [0.0, 0.0];
    let search = |th: f64| {
        ph.search_with_options(
            &cache,
            &q,
            10,
            &VectorIndexOptions::new(10, Some(th), accept_all::<f64>),
        )
        .unwrap()
    };
    assert_eq!(search(0.75), vec![1]);
    let mut high = search(1.5);
    high.sort_unstable();
    assert_eq!(high, vec![1, 2]);
    assert_eq!(search(0.6), vec![1], "the comparison is inclusive");
    assert!(search(0.5).is_empty());
}

#[test]
fn test_hnsw_distance_threshold_cosine() {
    let pred = "0-thresh_pred_c";
    let o = opts(&[("maxLevels", "1"), ("efSearch", "10"), ("metric", "cosine")]);
    let ph = PersistentHnsw::<f64>::new(pred, &o, 64).unwrap();
    let cache = synthetic(
        pred,
        &[(1, vec![1.0, 0.0]), (2, vec![0.8, 0.6])],
        &[(1, vec![vec![1, 2]]), (2, vec![vec![1]])],
    );
    let res = ph.search_with_options(
        &cache,
        &[1.0, 0.0],
        10,
        &VectorIndexOptions::new(10, Some(0.1), accept_all::<f64>),
    );
    assert_eq!(res, Ok(vec![1]));
}

#[test]
fn test_an_empty_index_reports_the_empty_index_result() {
    let txn = MapTxn::default();
    let ph = index("euclidean", "3");
    let qc = QueryCache::new(&txn, 1);
    for result in [
        ph.search(&qc, &[1.0], 3, accept_all::<f64>),
        ph.search_with_options(&qc, &[1.0], 3, &VectorIndexOptions::default()),
    ] {
        let e = result.unwrap_err();
        assert!(e.is_empty_index(), "{e}");
        assert_eq!(e.to_string(), "HNSW tree has no elements: Key not found");
    }
    assert_eq!(
        ph.search_with_uid(&qc, 7, 3, accept_all::<f64>),
        Ok(vec![]),
        "no vector, no neighbours"
    );
    assert_eq!(
        ph.search_with_uid_and_options(&qc, 7, 3, &VectorIndexOptions::default()),
        Ok(vec![])
    );
    // A cache whose find reports nothing without an error is Dgraph's plain empty-tree error.
    let e = ph
        .search(&MemoryCache::default(), &[1.0], 3, accept_all::<f64>)
        .unwrap_err();
    assert_eq!(e.0, HnswErrorEnum::EmptyTree(None));
}

/// Deviation: with a result count of 0 a layer search truncates its neighbours to none and Dgraph panics indexing
/// them; the port returns `NoNeighbour`. `search_with_options` stops before, as Dgraph does.
#[test]
fn test_a_result_count_of_zero() {
    let txn = MapTxn::default();
    let ph = index("euclidean", "1");
    for (uid, v) in [(3, [0.0, 1.0]), (6, [1.0, 1.0]), (9, [5.0, 5.0])] {
        txn.db
            .borrow_mut()
            .insert(key("0-a", uid), float64_array_as_bytes(&v));
        index("euclidean", "1")
            .insert(&TxnCache::new(&txn, 1), uid, &v, &mut || 0.5)
            .unwrap();
    }
    let qc = QueryCache::new(&txn, 1);
    let e = ph
        .search(&qc, &[1.0, 1.0], 0, accept_all::<f64>)
        .unwrap_err();
    assert_eq!(e.0, HnswErrorEnum::NoNeighbour);
    assert_eq!(
        ph.search_with_options(&qc, &[1.0, 1.0], 0, &VectorIndexOptions::default()),
        Ok(vec![])
    );
}

#[test]
fn test_a_search_reports_its_path_and_counters_and_honours_its_filter() {
    let txn = MapTxn::default();
    for (uid, v) in [(3, [0.0, 1.0]), (6, [1.0, 1.0]), (9, [5.0, 5.0])] {
        txn.db
            .borrow_mut()
            .insert(key("0-a", uid), float64_array_as_bytes(&v));
        index("euclidean", "2")
            .insert(&TxnCache::new(&txn, 1), uid, &v, &mut || 0.5)
            .unwrap();
    }
    let ph = index("euclidean", "2");
    let qc = QueryCache::new(&txn, 1);
    let r = ph
        .search_with_path(&qc, &[1.0, 1.1], 2, accept_all::<f64>)
        .unwrap();
    assert_eq!(r.neighbors(), &[6, 3]);
    assert_eq!(
        r.path().first(),
        Some(&3),
        "the path starts at the entry point"
    );
    assert!(r.metrics()["vector_distance_computations"] >= 1);
    assert!(r.metrics().contains_key("visited_vectors_level_1"));
    assert!(
        ph.search(&qc, &[1.0, 1.1], 2, accept_none::<f64>)
            .unwrap()
            .is_empty()
    );
    // The uid search returns the filtered neighbours too, as Dgraph's does, beyond the count asked for.
    assert_eq!(
        ph.search_with_uid(&qc, 6, 2, accept_none::<f64>)
            .unwrap()
            .len(),
        3
    );
}
