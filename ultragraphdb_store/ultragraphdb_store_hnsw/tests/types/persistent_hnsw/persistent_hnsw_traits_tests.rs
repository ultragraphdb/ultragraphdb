/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The index through the traits of `ultragraphdb_traits_index`, as index maintenance calls it: every method answers
//! as the index's own does.

use ultragraphdb_store_hnsw::{PersistentHnsw, QueryCache, TxnCache};
use ultragraphdb_traits_index::{
    OptionalIndexSupport, OptionalSearchOptions, VectorIndex, VectorIndexOptions, accept_all,
};

use crate::utils::helpers::{MapTxn, float64_array_as_bytes, key, opts};

const PRED: &str = "0-t";

fn index() -> PersistentHnsw<f64> {
    let o = opts(&[
        ("maxLevels", "3"),
        ("efConstruction", "8"),
        ("efSearch", "6"),
    ]);
    PersistentHnsw::new(PRED, &o, 64).unwrap()
}

fn points() -> Vec<(u64, Vec<f64>)> {
    (1..=20u64)
        .map(|uid| (uid, vec![uid as f64, (uid % 5) as f64]))
        .collect()
}

#[test]
fn test_an_insert_through_the_trait_writes_what_the_index_writes() {
    let (by_trait, by_index) = (MapTxn::default(), MapTxn::default());
    for (uid, v) in points() {
        for txn in [&by_trait, &by_index] {
            txn.db
                .borrow_mut()
                .insert(key(PRED, uid), float64_array_as_bytes(&v));
        }
        let draw = (uid % 7) as f64 / 7.0;
        let a = VectorIndex::insert(&index(), &by_trait, uid, &v, &mut || draw).unwrap();
        let b = index()
            .insert(&TxnCache::new(&by_index, 1), uid, &v, &mut || draw)
            .unwrap();
        assert_eq!(a, b, "uid {uid}");
    }
    assert_eq!(*by_trait.db.borrow(), *by_index.db.borrow());
}

#[test]
fn test_searches_through_the_traits_answer_as_the_index_does() {
    let txn = MapTxn::default();
    for (uid, v) in points() {
        txn.db
            .borrow_mut()
            .insert(key(PRED, uid), float64_array_as_bytes(&v));
        index()
            .insert(&TxnCache::new(&txn, 1), uid, &v, &mut || 0.5)
            .unwrap();
    }
    let ix = index();
    let qc = QueryCache::new(&txn, 1);
    let q = [7.2, 1.0];
    let opts = VectorIndexOptions::new(4, Some(3.0), accept_all::<f64>);
    assert_eq!(
        VectorIndex::search(&ix, &qc, &q, 3, accept_all::<f64>),
        ix.search(&qc, &q, 3, accept_all::<f64>)
    );
    assert_eq!(
        VectorIndex::search_with_uid(&ix, &qc, 7, 3, accept_all::<f64>),
        ix.search_with_uid(&qc, 7, 3, accept_all::<f64>)
    );
    assert_eq!(
        OptionalIndexSupport::search_with_path(&ix, &qc, &q, 3, accept_all::<f64>)
            .map(|r| r.neighbors().to_vec()),
        ix.search_with_path(&qc, &q, 3, accept_all::<f64>)
            .map(|r| r.neighbors().to_vec())
    );
    assert_eq!(
        OptionalSearchOptions::search_with_options(&ix, &qc, &q, 3, &opts),
        ix.search_with_options(&qc, &q, 3, &opts)
    );
    assert_eq!(
        OptionalSearchOptions::search_with_uid_and_options(&ix, &qc, 7, 3, &opts),
        ix.search_with_uid_and_options(&qc, 7, 3, &opts)
    );
    assert!(
        !VectorIndex::search(&ix, &qc, &q, 3, accept_all::<f64>)
            .unwrap()
            .is_empty()
    );
}
