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

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_tok::{VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_hnsw::{
    HnswError, HnswErrorEnum, HnswFloat, PersistentHnsw, QueryCache, TxnCache, encode_id_matrix,
};
use ultragraphdb_store_posting::{PostingError, PostingErrorEnum};
use ultragraphdb_store_sim::SimRng;
use ultragraphdb_traits_index::{
    CacheType, KeyValue, LocalCache, Metric, VectorIndexOptions, accept_all, accept_none,
};

use crate::utils::helpers::{
    InMemLocalCache, InMemTxn, MapTxn, MemoryCache, TsDbs, brute_force, draw,
    float64_array_as_bytes, id, ids, key, nums, opts, vec_bytes, vkeys,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn index(metric: &str, max_levels: &str) -> TestResult<PersistentHnsw<f64>> {
    let o = opts(&[
        ("maxLevels", max_levels),
        ("efConstruction", "16"),
        ("efSearch", "12"),
        ("metric", metric),
    ])?;
    Ok(PersistentHnsw::new(vkeys("0-a")?, &o, 64)?)
}

#[test]
fn test_basic_search_persistent_flat_storage() -> TestResult {
    for metric in ["euclidean", "cosine", "dotproduct"] {
        let dbs = TsDbs::default();
        let mut rng = SimRng::new(4);
        for (start, commit, uid, vec) in [
            (0, 1, 5, [0.1, 0.1, 0.1]),
            (11, 15, 123, [0.824, 0.319, 0.111]),
            (20, 37, 1, [0.3, 0.5, 0.7]),
        ] {
            dbs.set_all(&key("0-a", uid)?, &float64_array_as_bytes(&vec));
            let txn = InMemTxn::new(&dbs, start, commit);
            let _inserted: Vec<KeyValue> = index(metric, "5")?.insert(
                &TxnCache::new(&txn, start),
                id(uid)?,
                &vec,
                &mut || draw(&mut rng),
            )?;
        }
        for (read_ts, query, expected) in [
            (45, [0.3, 0.5, 0.7], vec![1]),
            (93, [0.824, 0.319, 0.111], vec![123]),
        ] {
            let lc = InMemLocalCache::new(&dbs, read_ts);
            let nns = index(metric, "5")?.search(
                &QueryCache::new(&lc, read_ts),
                &query,
                1,
                accept_all::<f64>,
            )?;
            assert_eq!(nums(&nns), expected, "{metric} at {read_ts}");
        }
    }
    Ok(())
}

#[test]
fn test_search_returns_correct_order_for_all_metrics() -> TestResult {
    for metric in ["euclidean", "cosine", "dotproduct"] {
        let dbs = TsDbs::default();
        let pred = format!("0-test{metric}");
        let o = opts(&[
            ("maxLevels", "1"),
            ("efConstruction", "16"),
            ("efSearch", "12"),
            ("metric", metric),
        ])?;
        for (start, uid, vec) in [
            (0, 10, [1.0, 0.0, 0.0]),
            (1, 20, [0.9, 0.1, 0.0]),
            (2, 30, [0.1, 0.9, 0.0]),
        ] {
            dbs.set_all(&key(&pred, uid)?, &float64_array_as_bytes(&vec));
            let txn = InMemTxn::new(&dbs, start, start + 1);
            let _inserted: Vec<KeyValue> = PersistentHnsw::<f64>::new(vkeys(&pred)?, &o, 64)?
                .insert(&TxnCache::new(&txn, start), id(uid)?, &vec, &mut || 0.5)?;
        }
        let lc = InMemLocalCache::new(&dbs, 10);
        let results = PersistentHnsw::<f64>::new(vkeys(&pred)?, &o, 64)?.search(
            &QueryCache::new(&lc, 10),
            &[1.0, 0.0, 0.0],
            2,
            accept_all::<f64>,
        )?;
        // By hand, from (1,0,0): uid 10 is 0 away, cosine 1, dot 1; uid 20 is sqrt 0.02 away, cosine 0.9/sqrt 0.82,
        // dot 0.9; uid 30 is sqrt 1.62 away, cosine 0.1/sqrt 0.82, dot 0.1. Go's test asserts only that 30 is not first.
        assert_eq!(nums(&results), vec![10, 20], "{metric}");
    }
    Ok(())
}

/// The predicate of a synthetic graph, its vectors, the entry point (the node numbered 1) and every node's stored
/// neighbour lists, each node by its number.
fn synthetic(
    pred: &str,
    vectors: &[(u64, Vec<f64>)],
    edges: &[(u64, Vec<Vec<u64>>)],
) -> TestResult<MemoryCache> {
    let mut cache = MemoryCache::default();
    for (uid, v) in vectors {
        let _replaced_vector: Option<Vec<u8>> = cache
            .data
            .insert(key(pred, *uid)?, float64_array_as_bytes(v));
    }
    let _replaced_entry: Option<Vec<u8>> = cache.data.insert(
        key(&format!("{pred}{VEC_ENTRY}"), 1)?,
        id(1)?.to_be_bytes().to_vec(),
    );
    for (uid, lists) in edges {
        let rows = lists
            .iter()
            .map(|row| Ok(ids(row)?))
            .collect::<TestResult<Vec<_>>>()?;
        let _replaced_lists: Option<Vec<u8>> = cache.data.insert(
            key(&format!("{pred}{VEC_KEYWORD}"), *uid)?,
            encode_id_matrix(&rows),
        );
    }
    Ok(cache)
}

#[test]
fn test_hnsw_search_ef_override_improves_recall() -> TestResult {
    let pred = "0-ef_override_test_pred";
    let o = opts(&[
        ("maxLevels", "2"),
        ("efSearch", "1"),
        ("metric", "euclidean"),
    ])?;
    let ph = PersistentHnsw::<f64>::new(vkeys(pred)?, &o, 64)?;
    assert_eq!(ph.keys().name(), pred);
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
    )?;
    let query = [0.0, 0.0, 0.12, 0.0];
    let narrow = ph.search_with_options(&cache, &query, 1, &VectorIndexOptions::default());
    assert_eq!(narrow, Ok(ids(&[200])?));
    let wide = ph.search_with_options(
        &cache,
        &query,
        1,
        &VectorIndexOptions::new(4, None, accept_all::<f64>),
    );
    assert_eq!(wide, Ok(ids(&[100])?));
    Ok(())
}

#[test]
fn test_hnsw_distance_threshold_euclidean() -> TestResult {
    let pred = "0-thresh_pred_e";
    let o = opts(&[
        ("maxLevels", "1"),
        ("efSearch", "10"),
        ("metric", "euclidean"),
    ])?;
    let ph = PersistentHnsw::<f64>::new(vkeys(pred)?, &o, 64)?;
    let cache = synthetic(
        pred,
        &[(1, vec![0.6, 0.0]), (2, vec![0.8, 0.0])],
        &[(1, vec![vec![1, 2]]), (2, vec![vec![1]])],
    )?;
    let q = [0.0, 0.0];
    let search = |th: f64| {
        ph.search_with_options(
            &cache,
            &q,
            10,
            &VectorIndexOptions::new(10, Some(th), accept_all::<f64>),
        )
    };
    assert_eq!(nums(&search(0.75)?), vec![1]);
    let mut high = nums(&search(1.5)?);
    high.sort_unstable();
    assert_eq!(high, vec![1, 2]);
    assert_eq!(nums(&search(0.6)?), vec![1], "the comparison is inclusive");
    assert!(search(0.5)?.is_empty());
    Ok(())
}

#[test]
fn test_hnsw_distance_threshold_cosine() -> TestResult {
    let pred = "0-thresh_pred_c";
    let o = opts(&[("maxLevels", "1"), ("efSearch", "10"), ("metric", "cosine")])?;
    let ph = PersistentHnsw::<f64>::new(vkeys(pred)?, &o, 64)?;
    let cache = synthetic(
        pred,
        &[(1, vec![1.0, 0.0]), (2, vec![0.8, 0.6])],
        &[(1, vec![vec![1, 2]]), (2, vec![vec![1]])],
    )?;
    let res = ph.search_with_options(
        &cache,
        &[1.0, 0.0],
        10,
        &VectorIndexOptions::new(10, Some(0.1), accept_all::<f64>),
    );
    assert_eq!(res, Ok(ids(&[1])?));
    Ok(())
}

#[test]
fn test_an_empty_index_reports_the_empty_index_result() -> TestResult {
    let txn = MapTxn::default();
    let ph = index("euclidean", "3")?;
    let qc = QueryCache::new(&txn, 1);
    for result in [
        ph.search(&qc, &[1.0], 3, accept_all::<f64>),
        ph.search_with_options(&qc, &[1.0], 3, &VectorIndexOptions::default()),
    ] {
        let e = result.err().context("the error of result")?;
        assert!(e.is_empty_index(), "{e}");
        assert_eq!(e.to_string(), "HNSW tree has no elements: Key not found");
    }
    assert_eq!(
        ph.search_with_uid(&qc, id(7)?, 3, accept_all::<f64>),
        Ok(vec![]),
        "no vector, no neighbours"
    );
    assert_eq!(
        ph.search_with_uid_and_options(&qc, id(7)?, 3, &VectorIndexOptions::default()),
        Ok(vec![])
    );
    // A cache whose find reports nothing without an error is Dgraph's plain empty-tree error.
    let e = ph
        .search(&MemoryCache::default(), &[1.0], 3, accept_all::<f64>)
        .err()
        .context("the error of ph .search(..)")?;
    assert_eq!(e.0, HnswErrorEnum::EmptyTree(None));
    Ok(())
}

/// Deviation: with a result count of 0 a layer search truncates its neighbours to none and Dgraph panics indexing
/// them; the port returns `NoNeighbour`. `search_with_options` stops before, as Dgraph does.
#[test]
fn test_a_result_count_of_zero() -> TestResult {
    let txn = MapTxn::default();
    let ph = index("euclidean", "1")?;
    for (uid, v) in [(3, [0.0, 1.0]), (6, [1.0, 1.0]), (9, [5.0, 5.0])] {
        txn.put(key("0-a", uid)?, float64_array_as_bytes(&v));
        let _inserted: Vec<KeyValue> =
            index("euclidean", "1")?.insert(&TxnCache::new(&txn, 1), id(uid)?, &v, &mut || 0.5)?;
    }
    let qc = QueryCache::new(&txn, 1);
    let e = ph
        .search(&qc, &[1.0, 1.0], 0, accept_all::<f64>)
        .err()
        .context("the error of ph .search(..)")?;
    assert_eq!(e.0, HnswErrorEnum::NoNeighbour);
    assert_eq!(
        ph.search_with_options(&qc, &[1.0, 1.0], 0, &VectorIndexOptions::default()),
        Ok(vec![])
    );
    Ok(())
}

#[test]
fn test_a_search_reports_its_path_and_counters_and_honours_its_filter() -> TestResult {
    let txn = MapTxn::default();
    for (uid, v) in [(3, [0.0, 1.0]), (6, [1.0, 1.0]), (9, [5.0, 5.0])] {
        txn.put(key("0-a", uid)?, float64_array_as_bytes(&v));
        let _inserted: Vec<KeyValue> =
            index("euclidean", "2")?.insert(&TxnCache::new(&txn, 1), id(uid)?, &v, &mut || 0.5)?;
    }
    let ph = index("euclidean", "2")?;
    let qc = QueryCache::new(&txn, 1);
    let r = ph.search_with_path(&qc, &[1.0, 1.1], 2, accept_all::<f64>)?;
    // By hand. A draw of 0.5 puts every node on level 1 alone, so level 0 holds the entry point, 3, without
    // neighbours: one distance, one visit, path [3]. Level 1 holds 3 -> [6, 9], 6 -> [3, 9], 9 -> [6, 3]; from 3
    // (1.005 away) the search scores 6 (0.1 away, kept and on the path) and 9 (5.59 away, worse than the 2 kept), then
    // pops 6, whose neighbours are visited: three distances and three visits, path [3, 6].
    assert_eq!(nums(r.neighbors()), vec![6, 3]);
    assert_eq!(nums(r.path()), vec![3, 3, 6]);
    assert_eq!(
        r.metrics().clone(),
        std::collections::HashMap::from([
            ("vector_distance_computations".to_string(), 4),
            ("visited_vectors_level_0".to_string(), 1),
            ("visited_vectors_level_1".to_string(), 3),
        ])
    );
    assert_eq!(
        ph.search(&qc, &[1.0, 1.1], 2, accept_none::<f64>),
        Ok(vec![])
    );
    // The uid search returns the filtered neighbours too, as Dgraph's does, beyond the count asked for: from 6, its
    // neighbours 3 (1 away) and 9 (5.66 away), filtered, do not count towards the 2 asked for.
    assert_eq!(
        ph.search_with_uid(&qc, id(6)?, 2, accept_none::<f64>),
        Ok(ids(&[6, 3, 9])?)
    );
    assert_eq!(
        ph.search_with_uid_and_options(
            &qc,
            id(6)?,
            2,
            &VectorIndexOptions::new(0, None, accept_none::<f64>)
        ),
        Ok(vec![]),
        "the options' uid search leaves the filtered out"
    );
    Ok(())
}

/// An index of `metric` with `levels` levels, an `efConstruction` of 16 and an `efSearch` of 12, holding `nodes`
/// inserted in order over a golden transaction; with one level every node is on it whatever the draw.
fn probe_index(
    metric: &str,
    levels: &str,
    nodes: &[(u64, Vec<f64>)],
) -> TestResult<(MapTxn, PersistentHnsw<f64>)> {
    let o = opts(&[
        ("maxLevels", levels),
        ("efConstruction", "16"),
        ("efSearch", "12"),
        ("metric", metric),
    ])?;
    let txn = MapTxn::default();
    for (uid, v) in nodes {
        txn.put(key("0-a", *uid)?, float64_array_as_bytes(v));
        let _inserted: Vec<KeyValue> = PersistentHnsw::<f64>::new(vkeys("0-a")?, &o, 64)?.insert(
            &TxnCache::new(&txn, 1),
            id(*uid)?,
            v,
            &mut || 0.5,
        )?;
    }
    Ok((txn, PersistentHnsw::new(vkeys("0-a")?, &o, 64)?))
}

fn options(ef: usize, threshold: Option<f64>) -> VectorIndexOptions<f64> {
    VectorIndexOptions::new(ef, threshold, accept_all::<f64>)
}

/// Five vectors, three of them equal: every metric ties uids 3, 6 and 12 against the query (1, 0).
fn duplicates() -> Vec<(u64, Vec<f64>)> {
    vec![
        (3, vec![1.0, 0.0]),
        (6, vec![1.0, 0.0]),
        (9, vec![0.0, 1.0]),
        (12, vec![1.0, 0.0]),
        (15, vec![-1.0, 0.0]),
    ]
}

/// Ties keep the order the search met them in, and a count beyond the index returns every vector: every result is Go
/// 1.27's for the same inserts and searches (a probe of Dgraph's tok/hnsw over the golden transaction).
#[test]
fn test_ties_and_a_count_beyond_the_index_rank_as_gos() -> TestResult {
    for metric in ["euclidean", "cosine", "dotproduct"] {
        let (txn, ix) = probe_index(metric, "1", &duplicates())?;
        let qc = QueryCache::new(&txn, 1);
        let q = [1.0, 0.0];
        let search = |k: usize| ix.search(&qc, &q, k, accept_all::<f64>);
        // Go 1.27 probe.
        assert_eq!(nums(&search(1)?), vec![3], "{metric}");
        assert_eq!(nums(&search(2)?), vec![3, 6], "{metric}");
        assert_eq!(nums(&search(5)?), vec![3, 6, 12, 9, 15], "{metric}");
        assert_eq!(nums(&search(9)?), vec![3, 6, 12, 9, 15], "{metric}: k > n");
        assert_eq!(
            ix.search(&qc, &[0.0, 1.0], 9, accept_all::<f64>),
            Ok(ids(&[9, 3, 6, 12, 15])?),
            "{metric}"
        );
        assert_eq!(
            ix.search_with_options(&qc, &q, 9, &options(0, None)),
            Ok(ids(&[3, 6, 12, 9, 15])?),
            "{metric}"
        );
        assert_eq!(
            ix.search_with_uid(&qc, id(6)?, 9, accept_all::<f64>),
            Ok(ids(&[6, 3, 12, 9, 15])?),
            "{metric}"
        );
        assert_eq!(
            ix.search_with_uid_and_options(&qc, id(6)?, 9, &options(0, None)),
            Ok(ids(&[6, 3, 12, 9, 15])?),
            "{metric}"
        );
    }
    Ok(())
}

/// A query of another dimension than the index's vectors fails with the metric's `DifferentLengths`, Go's text.
#[test]
fn test_a_query_of_another_dimension_is_refused() -> TestResult {
    for (metric, func_name) in [
        ("euclidean", "euclidean distance"),
        ("cosine", "cosine distance"),
        ("dotproduct", "dot product"),
    ] {
        let (txn, ix) = probe_index(metric, "1", &duplicates())?;
        let qc = QueryCache::new(&txn, 1);
        for e in [
            ix.search(&qc, &[1.0, 0.0, 0.0], 2, accept_all::<f64>)
                .err()
                .context("the error of ix.search(..)")?,
            ix.search_with_options(&qc, &[1.0], 2, &options(0, None))
                .err()
                .context("the error of ix.search_with_options(..)")?,
        ] {
            assert_eq!(e.0, HnswErrorEnum::DifferentLengths(func_name));
            // Go 1.27 probe.
            assert_eq!(
                e.to_string(),
                format!("can not compute {func_name} on vectors of different lengths")
            );
        }
    }
    Ok(())
}

/// The threshold is a euclidean distance, inclusive; a cosine distance, `1 - similarity`, inclusive; and ignored under
/// dot product. From (1, 0): uid 9 is sqrt 2 away with cosine distance 1, uid 15 is 2 away with cosine distance 2.
#[test]
fn test_the_threshold_by_metric() -> TestResult {
    // (metric, threshold, Go 1.27 probe's result)
    for (metric, threshold, expected) in [
        ("euclidean", 0.0, vec![3, 6, 12]),
        ("euclidean", 1.0, vec![3, 6, 12]),
        ("euclidean", 1.5, vec![3, 6, 12, 9]),
        ("cosine", 0.0, vec![3, 6, 12]),
        ("cosine", 1.0, vec![3, 6, 12, 9]),
        ("dotproduct", 0.0, vec![3, 6, 12, 9, 15]),
        ("dotproduct", 1.0, vec![3, 6, 12, 9, 15]),
    ] {
        let (txn, ix) = probe_index(metric, "1", &duplicates())?;
        let qc = QueryCache::new(&txn, 1);
        assert_eq!(
            ix.search_with_options(&qc, &[1.0, 0.0], 9, &options(0, Some(threshold)))
                .map(|r| nums(&r)),
            Ok(expected),
            "{metric} within {threshold}"
        );
    }
    Ok(())
}

/// A count of 0: the plain and uid searches keep their start, as Dgraph's do when no neighbour beats it; the searches
/// with options return nothing. Go 1.27 probe.
#[test]
fn test_a_count_of_zero_by_search() -> TestResult {
    for metric in ["euclidean", "cosine", "dotproduct"] {
        let (txn, ix) = probe_index(metric, "1", &duplicates())?;
        let qc = QueryCache::new(&txn, 1);
        let q = [1.0, 0.0];
        assert_eq!(ix.search(&qc, &q, 0, accept_all::<f64>), Ok(ids(&[3])?));
        assert_eq!(
            ix.search_with_options(&qc, &q, 0, &options(0, None)),
            Ok(vec![])
        );
        assert_eq!(
            ix.search_with_uid(&qc, id(6)?, 0, accept_all::<f64>),
            Ok(ids(&[6])?)
        );
        assert_eq!(
            ix.search_with_uid_and_options(&qc, id(6)?, 0, &options(0, None)),
            Ok(vec![])
        );
        assert_eq!(
            ix.search_with_uid_and_options(&qc, id(6)?, 0, &options(3, None)),
            Ok(vec![])
        );
    }
    Ok(())
}

/// A zero vector's cosine similarity is 0/0, NaN, which no score beats and which beats none, and which no threshold
/// excludes: every result is Go 1.27's (probe).
#[test]
fn test_a_zero_vector_under_cosine() -> TestResult {
    let (txn, ix) = probe_index(
        "cosine",
        "1",
        &[
            (3, vec![1.0, 0.0]),
            (6, vec![0.0, 0.0]),
            (9, vec![0.0, 1.0]),
            (12, vec![0.6, 0.8]),
        ],
    )?;
    let qc = QueryCache::new(&txn, 1);
    assert_eq!(
        ix.search(&qc, &[1.0, 0.0], 4, accept_all::<f64>),
        Ok(ids(&[3, 6, 12, 9])?)
    );
    assert_eq!(
        ix.search(&qc, &[0.0, 0.0], 4, accept_all::<f64>),
        Ok(ids(&[3, 6, 12, 9])?)
    );
    assert_eq!(
        ix.search_with_options(&qc, &[1.0, 0.0], 4, &options(0, Some(0.5))),
        Ok(ids(&[3, 6, 12])?)
    );
    assert_eq!(
        ix.search_with_options(&qc, &[0.0, 0.0], 4, &options(0, Some(0.5))),
        Ok(ids(&[3, 6, 12, 9])?)
    );
    assert_eq!(
        ix.search_with_uid(&qc, id(6)?, 4, accept_all::<f64>),
        Ok(ids(&[6, 3, 9, 12])?)
    );
    Ok(())
}

/// NaN and infinite components score NaN or infinity and rank where Go's insertion puts them. Go 1.27 probe.
#[test]
fn test_non_finite_components_rank_as_gos() -> TestResult {
    let nodes = [
        (3, vec![1.0, 0.0]),
        (6, vec![f64::NAN, 0.0]),
        (9, vec![0.0, 1.0]),
        (12, vec![f64::INFINITY, 0.0]),
        (15, vec![2.0, 2.0]),
    ];
    // (metric, results for the queries (1, 0), (0, 1) and (NaN, 1))
    for (metric, expected) in [
        (
            "euclidean",
            [[3, 6, 9, 15, 12], [3, 6, 9, 15, 12], [3, 6, 9, 15, 12]],
        ),
        (
            "cosine",
            [[3, 6, 15, 9, 12], [3, 6, 9, 15, 12], [3, 6, 15, 9, 12]],
        ),
        (
            "dotproduct",
            [[3, 6, 12, 15, 9], [3, 6, 12, 15, 9], [3, 6, 12, 15, 9]],
        ),
    ] {
        let (txn, ix) = probe_index(metric, "1", &nodes)?;
        let qc = QueryCache::new(&txn, 1);
        for (q, want) in [[1.0, 0.0], [0.0, 1.0], [f64::NAN, 1.0]]
            .iter()
            .zip(expected)
        {
            assert_eq!(
                ix.search(&qc, q, 5, accept_all::<f64>).map(|r| nums(&r)),
                Ok(want.to_vec()),
                "{metric} {q:?}"
            );
        }
    }
    Ok(())
}

/// Every dot product below zero still ranks highest first: (1,1) scores -3, -4, -1 and -8 against uids 3, 6, 9 and 12.
#[test]
fn test_negative_dot_products_rank_highest_first() -> TestResult {
    let (txn, ix) = probe_index(
        "dotproduct",
        "1",
        &[
            (3, vec![-1.0, -2.0]),
            (6, vec![-3.0, -1.0]),
            (9, vec![-0.5, -0.5]),
            (12, vec![-4.0, -4.0]),
        ],
    )?;
    let qc = QueryCache::new(&txn, 1);
    let q = [1.0, 1.0];
    // By hand, as the Go 1.27 probe returns them.
    assert_eq!(ix.search(&qc, &q, 2, accept_all::<f64>), Ok(ids(&[9, 3])?));
    assert_eq!(
        ix.search(&qc, &q, 4, accept_all::<f64>),
        Ok(ids(&[9, 3, 6, 12])?)
    );
    assert_eq!(
        ix.search_with_options(&qc, &q, 4, &options(0, Some(-100.0))),
        Ok(ids(&[9, 3, 6, 12])?),
        "dot product ignores a threshold"
    );
    Ok(())
}

/// An index of one vector over three levels answers every search with it, a threshold permitting. Go 1.27 probe.
#[test]
fn test_one_vector() -> TestResult {
    let (txn, ix) = probe_index("euclidean", "3", &[(7, vec![0.5, 0.5])])?;
    let qc = QueryCache::new(&txn, 1);
    assert_eq!(
        ix.search(&qc, &[0.5, 0.5], 3, accept_all::<f64>),
        Ok(ids(&[7])?)
    );
    assert_eq!(
        ix.search(&qc, &[9.0, 9.0], 3, accept_all::<f64>),
        Ok(ids(&[7])?)
    );
    // (9,9) is 8.5 * sqrt 2 from (0.5,0.5), beyond 1.
    assert_eq!(
        ix.search_with_options(&qc, &[9.0, 9.0], 3, &options(0, Some(1.0))),
        Ok(vec![])
    );
    assert_eq!(
        ix.search_with_uid(&qc, id(7)?, 3, accept_all::<f64>),
        Ok(ids(&[7])?)
    );
    assert_eq!(
        ix.search_with_uid_and_options(&qc, id(7)?, 3, &options(0, None)),
        Ok(ids(&[7])?)
    );
    Ok(())
}

/// Inserts 30 random vectors of `T` under `metric` into a 3-level index whose breadths exceed the set, and searches
/// with options for each count: the result is the exact nearest by brute force, the oracle's.
fn equals_brute_force<T: HnswFloat>(metric: Metric, seed: u64) -> TestResult {
    let o = opts(&[
        ("maxLevels", "3"),
        ("efConstruction", "32"),
        ("efSearch", "40"),
        ("metric", metric.name()),
    ])?;
    let mut rng = SimRng::new(seed);
    let rand_vec = |source: &mut SimRng| -> Vec<f64> {
        (0..4)
            .map(|_| T::from_f64(draw(source) * 2.0 - 1.0).to_f64())
            .collect()
    };
    let txn = MapTxn::default();
    let mut vectors = Vec::new();
    for uid in 1..=30u64 {
        let v = rand_vec(&mut rng);
        let tv: Vec<T> = v.iter().map(|&x| T::from_f64(x)).collect();
        txn.put(key("0-a", uid)?, vec_bytes(&tv));
        let _inserted: Vec<KeyValue> = PersistentHnsw::<T>::new(vkeys("0-a")?, &o, T::BITS)?
            .insert(&TxnCache::new(&txn, 1), id(uid)?, &tv, &mut || {
                draw(&mut rng)
            })?;
        vectors.push((id(uid)?, v));
    }
    let ix = PersistentHnsw::<T>::new(vkeys("0-a")?, &o, T::BITS)?;
    let qc = QueryCache::new(&txn, 1);
    for _ in 0..5 {
        let q = rand_vec(&mut rng);
        let tq: Vec<T> = q.iter().map(|&x| T::from_f64(x)).collect();
        for k in [1, 5, 30, 35] {
            assert_eq!(
                ix.search_with_options(&qc, &tq, k, &VectorIndexOptions::default()),
                Ok(brute_force(metric, &vectors, &q, k)),
                "{metric:?} at {} bits, k {k}",
                T::BITS
            );
        }
    }
    Ok(())
}

#[test]
fn test_small_sets_return_the_exact_top_k_for_each_metric_and_width() -> TestResult {
    for metric in [Metric::Euclidean, Metric::Cosine, Metric::DotProduct] {
        equals_brute_force::<f32>(metric, 21)?;
        equals_brute_force::<f64>(metric, 22)?;
    }
    Ok(())
}

/// A cache whose `find` fails, as a store's failed scan.
struct FailingFind(MapTxn);

impl CacheType for FailingFind {
    type Error = HnswError;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, HnswError> {
        LocalCache::get(&self.0, key)
    }

    fn ts(&self) -> u64 {
        1
    }

    fn find(
        &self,
        prefix: &[u8],
        _: impl Fn(&[u8]) -> bool,
    ) -> Result<Option<SystemId>, HnswError> {
        Err(PostingError::store(StoreError::read_failed(prefix.to_vec(), "injected".into())).into())
    }
}

/// What a search refuses, each naming its key: a vector that does not fill its float width, met by the scan for an
/// entry point or as a neighbour; an entry point of other than 8 bytes; a node without the level searched; and a scan
/// that fails, which is no empty index.
#[test]
fn test_a_search_refuses_what_does_not_decode() -> TestResult {
    let short = vec![0u8; 12];
    let txn = MapTxn::default();
    txn.put(key("0-a", 3)?, short.clone());
    let e = index("euclidean", "2")?
        .search(&QueryCache::new(&txn, 1), &[1.0], 1, accept_all::<f64>)
        .err()
        .context("the error of index(\"euclidean\", \"2\") .search(..)")?;
    assert_eq!(
        e,
        HnswError::corrupted(
            &key("0-a", 3)?,
            "a vector whose length is not a multiple of its float width"
        )
    );

    let entry_txn = MapTxn::default();
    entry_txn.put(key("0-a", 3)?, float64_array_as_bytes(&[1.0]));
    entry_txn.put(key(&format!("0-a{VEC_ENTRY}"), 1)?, vec![0, 0, 3]);
    let bad_entry = index("euclidean", "2")?
        .search(
            &QueryCache::new(&entry_txn, 1),
            &[1.0],
            1,
            accept_all::<f64>,
        )
        .err()
        .context("the error of index(\"euclidean\", \"2\") .search(..)")?;
    assert_eq!(
        bad_entry,
        HnswError::corrupted(
            &key(&format!("0-a{VEC_ENTRY}"), 1)?,
            "an entry point of 3 bytes"
        )
    );

    // Uid 1 is the entry point; uid 2, its neighbour, holds lists of one level where the index has two.
    let mut cache = synthetic(
        "0-a",
        &[(1, vec![0.0, 0.0]), (2, vec![1.0, 0.0])],
        &[(1, vec![vec![2], vec![2]]), (2, vec![vec![1]])],
    )?;
    let level_missing = index("euclidean", "2")?
        .search(&cache, &[1.0, 0.0], 1, accept_all::<f64>)
        .err()
        .context("the error of index(\"euclidean\", \"2\") .search(..)")?;
    assert_eq!(
        level_missing.0,
        HnswErrorEnum::LevelMissing {
            uid: id(2)?,
            level: 1,
            levels: 1
        },
        "where Dgraph panics"
    );
    let _previous: Option<Vec<u8>> = cache.data.insert(key("0-a", 2)?, short);
    let short_neighbour = index("euclidean", "2")?
        .search(&cache, &[1.0, 0.0], 1, accept_all::<f64>)
        .err()
        .context("the error of index(\"euclidean\", \"2\") .search(..)")?;
    assert_eq!(
        short_neighbour,
        HnswError::corrupted(&key("0-a", 2)?, "a vector of 12 bytes")
    );
    assert_a_failed_scan_is_no_empty_index()?;
    Ok(())
}

/// A scan for an entry point that fails is the read's failure, not an empty index.
fn assert_a_failed_scan_is_no_empty_index() -> TestResult {
    let e = index("euclidean", "2")?
        .search(
            &FailingFind(MapTxn::default()),
            &[1.0],
            1,
            accept_all::<f64>,
        )
        .err()
        .context("the error of index(\"euclidean\", \"2\") .search(..)")?;
    assert!(
        matches!(
            &e.0,
            HnswErrorEnum::Posting(PostingError(PostingErrorEnum::Store(StoreError(
                StoreErrorEnum::ReadFailed { .. }
            ))))
        ),
        "{e}"
    );
    assert!(!e.is_empty_index());
    Ok(())
}

/// A uid whose key holds no value, as a cache that reads absence as no bytes reports it, is Dgraph's `errNilVector`,
/// which the uid searches return, as Dgraph's do; an absence the cache reports as an error is no neighbours.
#[test]
fn test_a_uid_holding_no_vector_by_cache() -> TestResult {
    let ph = index("euclidean", "1")?;
    let cache = MemoryCache::default();
    for e in [
        ph.search_with_uid(&cache, id(7)?, 3, accept_all::<f64>)
            .err()
            .context("the error of ph.search_with_uid(..)")?,
        ph.search_with_uid_and_options(&cache, id(7)?, 3, &options(0, None))
            .err()
            .context("the error of ph.search_with_uid_and_options(..)")?,
    ] {
        assert_eq!(e.0, HnswErrorEnum::NilVector(None));
        assert_eq!(e.to_string(), "nil vector returned");
        assert!(e.is_nil_vector() && !e.is_fetching_posting_list());
    }
    Ok(())
}

/// The candidates a layer search keeps exceed its breadth by one before Dgraph drops the heap's last leaf
/// (`candidateHeap.Len() > expectedNeighbors`): on this stored graph, found among 12,000 random ones, a heap capped at
/// the breadth itself loses the path to 2 and returns 9. Go 1.27's search of the same lists (a probe over the golden
/// transaction) returns [7, 5, 2] for 3, though 9 is nearer than 2: the search is approximate, and this is Dgraph's.
#[test]
fn test_the_candidate_heap_keeps_one_beyond_the_breadth() -> TestResult {
    let vectors: Vec<(u64, Vec<f64>)> = [
        (62.0, 26.0),
        (72.0, 51.0),
        (9.0, 50.0),
        (47.0, 45.0),
        (71.0, 87.0),
        (91.0, 44.0),
        (74.0, 88.0),
        (58.0, 9.0),
        (64.0, 96.0),
        (81.0, 54.0),
    ]
    .iter()
    .zip(1u64..)
    .map(|(&(x, y), uid)| (uid, vec![x, y]))
    .collect();
    let lists: [&[u64]; 10] = [
        &[5, 8, 7, 6],
        &[3, 9],
        &[7],
        &[8],
        &[8, 3, 1, 8],
        &[4, 1, 2],
        &[4, 2],
        &[1, 4, 6],
        &[],
        &[1],
    ];
    let edges: Vec<(u64, Vec<Vec<u64>>)> = lists
        .iter()
        .zip(1u64..)
        .map(|(l, uid)| (uid, vec![l.to_vec()]))
        .collect();
    let cache = synthetic("0-a", &vectors, &edges)?;
    let ph = index("euclidean", "1")?;
    let q = [79.0, 81.0];
    // Go 1.27 probe.
    for (k, expected) in [
        (1, vec![7]),
        (2, vec![7, 5]),
        (3, vec![7, 5, 2]),
        (4, vec![7, 5, 2, 6]),
    ] {
        assert_eq!(
            ph.search(&cache, &q, k, accept_all::<f64>)
                .map(|r| nums(&r)),
            Ok(expected),
            "k {k}"
        );
    }
    Ok(())
}

/// A search whose entry point lost its vector, or whose index holds no entry point, starts from the first node in key
/// order holding a vector (Dgraph's `PickStartNode` and `calculateNewEntryVec`) and still answers exactly. By hand,
/// on a line: 3 at 0, 6 at 1, 9 at 2, 12 at 3; 3 loses its vector; from (2.2, 0) the nearest are 9 (0.2 away), 12
/// (0.8) and 6 (1.2), the brute-force order.
#[test]
fn test_a_search_from_a_lost_entry_point_starts_at_the_first_vector() -> TestResult {
    let nodes = [
        (3, vec![0.0, 0.0]),
        (6, vec![1.0, 0.0]),
        (9, vec![2.0, 0.0]),
        (12, vec![3.0, 0.0]),
    ];
    for levels in ["1", "3"] {
        let (txn, ix) = probe_index("euclidean", levels, &nodes)?;
        txn.remove(&key("0-a", 3)?);
        let q = [2.2, 0.0];
        let live: Vec<(SystemId, Vec<f64>)> = nodes[1..]
            .iter()
            .map(|(n, v)| Ok((id(*n)?, v.clone())))
            .collect::<TestResult<_>>()?;
        assert_eq!(
            brute_force(Metric::Euclidean, &live, &q, 3),
            ids(&[9, 12, 6])?
        );
        let qc = QueryCache::new(&txn, 1);
        assert_eq!(
            ix.search(&qc, &q, 3, accept_all::<f64>),
            Ok(ids(&[9, 12, 6])?),
            "{levels} levels"
        );
        assert_eq!(
            ix.search_with_options(&qc, &q, 3, &options(0, None)),
            Ok(ids(&[9, 12, 6])?),
            "{levels} levels"
        );
        // Without any entry point the search starts from the same node.
        txn.remove(&key(&format!("0-a{VEC_ENTRY}"), 1)?);
        let qc_no_entry = QueryCache::new(&txn, 1);
        assert_eq!(
            ix.search(&qc_no_entry, &q, 3, accept_all::<f64>),
            Ok(ids(&[9, 12, 6])?),
            "{levels} levels, no entry point"
        );
    }
    Ok(())
}

/// A cache whose `find` offers every value under the prefix to its filter, an empty one included, as Dgraph's
/// in-memory doubles do; its reads are the golden transaction's.
struct EveryValue(MapTxn);

impl CacheType for EveryValue {
    type Error = HnswError;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, HnswError> {
        LocalCache::get(&self.0, key)
    }

    fn ts(&self) -> u64 {
        1
    }

    fn find(
        &self,
        prefix: &[u8],
        filter: impl Fn(&[u8]) -> bool,
    ) -> Result<Option<SystemId>, HnswError> {
        let keys = vkeys("0-a").map_err(|_| HnswError::key_not_found())?;
        let db = self.0.db.borrow();
        db.iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .find(|(_, v)| filter(v))
            .and_then(|(k, _)| keys.node_of(k))
            .ok_or_else(HnswError::key_not_found)
            .map(Some)
    }
}

/// The scan for a new entry point passes over a node holding no elements, as Dgraph's `calculateNewEntryVec` takes the
/// first value that decodes to a non-empty vector: uid 2's value is empty, so the search starts from 5, the only vector,
/// and finds it.
#[test]
fn test_the_entry_scan_passes_over_an_empty_vector() -> TestResult {
    let txn = MapTxn::default();
    txn.put(key("0-a", 2)?, Vec::new());
    txn.put(key("0-a", 5)?, float64_array_as_bytes(&[1.0, 0.0]));
    let cache = EveryValue(txn);
    assert_eq!(
        index("euclidean", "1")?.search(&cache, &[1.0, 0.0], 1, accept_all::<f64>),
        Ok(ids(&[5])?)
    );
    Ok(())
}

/// A query equal to the start's vector skips the levels above the bottom, as Dgraph's `SearchWithPath` breaks on
/// `isEqual`: the path and counters hold the bottom level alone. By hand, on the graph of the test above: the query
/// (0, 1) is the entry point 3's vector; on level 1 the search scores 6 (1 away, kept) and 9 (sqrt 41 away, not), then
/// pops 6, whose neighbours are visited: three distances, three visits, path [3].
#[test]
fn test_a_query_equal_to_the_start_searches_the_bottom_level_alone() -> TestResult {
    let txn = MapTxn::default();
    for (uid, v) in [(3, [0.0, 1.0]), (6, [1.0, 1.0]), (9, [5.0, 5.0])] {
        txn.put(key("0-a", uid)?, float64_array_as_bytes(&v));
        let _inserted: Vec<KeyValue> =
            index("euclidean", "2")?.insert(&TxnCache::new(&txn, 1), id(uid)?, &v, &mut || 0.5)?;
    }
    let r = index("euclidean", "2")?.search_with_path(
        &QueryCache::new(&txn, 1),
        &[0.0, 1.0],
        2,
        accept_all::<f64>,
    )?;
    assert_eq!(nums(r.neighbors()), vec![3, 6]);
    assert_eq!(nums(r.path()), vec![3]);
    assert_eq!(
        r.metrics().clone(),
        std::collections::HashMap::from([
            ("vector_distance_computations".to_string(), 3),
            ("visited_vectors_level_1".to_string(), 3),
        ])
    );
    Ok(())
}

/// A query equal to the start's vector skips the levels above the bottom in every search, as Dgraph's `isEqual` break
/// does, even where a level above would find a better start: under dot product the entry point 1 at (1, 0) scores 1
/// against the query (1, 0), while 2 at (5, 0), its neighbour on level 0 alone, scores 5. The bottom level holds no
/// edges, so each search returns its start: 1, Dgraph's answer (a search of the upper level would return 2).
#[test]
fn test_a_query_equal_to_the_start_keeps_the_start_under_dot_product() -> TestResult {
    let cache = synthetic(
        "0-a",
        &[(1, vec![1.0, 0.0]), (2, vec![5.0, 0.0])],
        &[(1, vec![vec![2], vec![]]), (2, vec![vec![1], vec![]])],
    )?;
    let ph = index("dotproduct", "2")?;
    let q = [1.0, 0.0];
    assert_eq!(ph.search(&cache, &q, 1, accept_all::<f64>), Ok(ids(&[1])?));
    assert_eq!(
        ph.search_with_options(&cache, &q, 1, &options(0, None)),
        Ok(ids(&[1])?)
    );
    Ok(())
}

/// What the uid searches refuse, each naming its key: a query vector that does not fill its float width, and the
/// query node's neighbour lists that do not decode.
#[test]
fn test_the_uid_searches_refuse_what_does_not_decode() -> TestResult {
    let corrupted_at = |e: &HnswError, at: &[u8]| {
        matches!(
            &e.0,
            HnswErrorEnum::Posting(PostingError(PostingErrorEnum::Store(StoreError(
                StoreErrorEnum::Corrupted { key, .. }
            )))) if key.as_slice() == at
        )
    };
    let ph = index("euclidean", "1")?;
    let txn = MapTxn::default();
    let vector = key("0-a", 7)?;
    txn.put(vector.clone(), vec![0u8; 12]);
    let lists = key(&format!("0-a{VEC_KEYWORD}"), 6)?;
    txn.put(key("0-a", 6)?, float64_array_as_bytes(&[1.0, 1.0]));
    txn.put(lists.clone(), vec![1u8, 2, 3]);
    let qc = QueryCache::new(&txn, 1);
    for (uid, at) in [(7, &vector), (6, &lists)] {
        for e in [
            ph.search_with_uid(&qc, id(uid)?, 3, accept_all::<f64>)
                .err()
                .context("the error of ph.search_with_uid(..)")?,
            ph.search_with_uid_and_options(&qc, id(uid)?, 3, &VectorIndexOptions::default())
                .err()
                .context("the error of ph.search_with_uid_and_options(..)")?,
        ] {
            assert!(corrupted_at(&e, at), "{uid}: {e}");
        }
    }
    Ok(())
}

/// The options' search refuses a neighbour met on an upper level whose vector does not fill its float width, naming
/// its key, as the plain search does.
#[test]
fn test_the_options_search_refuses_a_short_vector_on_an_upper_level() -> TestResult {
    let mut cache = synthetic(
        "0-a",
        &[(1, vec![0.0, 0.0])],
        &[(1, vec![vec![2], vec![2]]), (2, vec![vec![1], vec![1]])],
    )?;
    let _absent: Option<Vec<u8>> = cache.data.insert(key("0-a", 2)?, vec![0u8; 12]);
    let e = index("euclidean", "2")?
        .search_with_options(&cache, &[1.0, 0.0], 1, &options(1, None))
        .err()
        .context("the error of search_with_options(..)")?;
    assert_eq!(
        e,
        HnswError::corrupted(&key("0-a", 2)?, "a vector of 12 bytes")
    );
    Ok(())
}

/// A layer search stops at a candidate worse than every neighbour kept. With a breadth of 1, the entry point 1 (10
/// away) yields 2 (5 away) and 3 (3 away), and 3 is kept; 3 holds no lists, so the search moves on to 2, which cannot
/// improve on 3, and stops there: 3 is the one neighbour.
#[test]
fn test_a_layer_search_stops_at_a_candidate_that_cannot_improve() -> TestResult {
    let cache = synthetic(
        "0-a",
        &[
            (1, vec![10.0, 0.0]),
            (2, vec![5.0, 0.0]),
            (3, vec![3.0, 0.0]),
        ],
        &[(1, vec![vec![2, 3]]), (2, vec![vec![4]])],
    )?;
    assert_eq!(
        index("euclidean", "1")?.search_with_options(&cache, &[0.0, 0.0], 1, &options(1, None)),
        Ok(ids(&[3])?)
    );
    Ok(())
}
