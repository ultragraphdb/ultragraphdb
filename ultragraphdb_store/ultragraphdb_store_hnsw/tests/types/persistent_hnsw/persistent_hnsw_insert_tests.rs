/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw_test.go (TestFlatEntryInsertToPersistentFlatStorage,
 * TestNonflatEntryInsertToPersistentFlatStorage, TestInsertHelper_NoDuplicateNeighborEdges). Changed for
 * ultragraphdb: translated to Rust; every insert builds its index, as Dgraph's index maintenance does, where Go's
 * tests share one index, and its caches, across their inserts; the levels drawn from a seeded source; added: the
 * deviations where Dgraph takes a failed read for absence, and what the decoders refuse.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

use ultragraphdb_core_tok::{VEC_DEAD, VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_hnsw::{
    HnswError, HnswErrorEnum, PersistentHnsw, TxnCache, bytes_as_float_array, bytes_to_uint64,
    decode_uint64_matrix, encode_uint64_matrix,
};
use ultragraphdb_store_posting::{PostingError, PostingErrorEnum};
use ultragraphdb_store_sim::SimRng;
use ultragraphdb_traits_index::{KeyValue, Txn};

use crate::utils::helpers::{
    InMemTxn, MapTxn, TsDbs, draw, edge_names, float64_array_as_bytes, key, opts,
};

const PRED: &str = "0-a";

/// Dgraph's `flatPhs[0]`: 5 levels, an `efConstruction` of 16, an `efSearch` of 12, euclidean, float64.
fn flat_ph() -> PersistentHnsw<f64> {
    let o = opts(&[
        ("maxLevels", "5"),
        ("efConstruction", "16"),
        ("efSearch", "12"),
    ]);
    PersistentHnsw::new(PRED, &o, 64).unwrap()
}

/// Sets `uid`'s vector at every timestamp and inserts it in a transaction from `start_ts` to `commit_ts`.
fn insert_at(
    dbs: &TsDbs,
    start_ts: u64,
    commit_ts: u64,
    uid: u64,
    vec: &[f64],
    rng: &mut SimRng,
) -> Result<Vec<KeyValue>, HnswError> {
    dbs.set_all(&key(PRED, uid), float64_array_as_bytes(vec));
    let txn = InMemTxn::new(dbs, start_ts, commit_ts);
    flat_ph().insert(&TxnCache::new(&txn, start_ts), uid, vec, &mut || draw(rng))
}

#[test]
fn test_flat_entry_insert_to_persistent_flat_storage() {
    let mut rng = SimRng::new(1);
    for (start, commit, uid, vec) in [
        (12, 40, 123, [0.824, 0.319, 0.111]),
        (11, 37, 1, [0.3, 0.5, 0.7]),
        (0, 1, 5, [0.1, 0.1, 0.1]),
    ] {
        let dbs = TsDbs::default();
        let edges = insert_at(&dbs, start, commit, uid, &vec, &mut rng).unwrap();
        let vkey = key(PRED, uid);
        assert_eq!(
            dbs.get(0, &vkey),
            dbs.get(99, &vkey),
            "the vector is unchanged"
        );
        assert_eq!(
            edge_names(&edges),
            vec![
                format!("0-a__vector__{uid}"),
                "0-a__vector_entry_1".to_string()
            ]
        );
        let entry = dbs.get(99, &key(&format!("{PRED}{VEC_ENTRY}"), 1)).unwrap();
        assert_eq!(bytes_to_uint64(&entry), Some(uid));
    }
}

#[test]
fn test_nonflat_entry_insert_to_persistent_flat_storage() {
    let mut rng = SimRng::new(2);
    let dbs = TsDbs::default();
    insert_at(&dbs, 0, 1, 5, &[0.1, 0.1, 0.1], &mut rng).unwrap();
    for (start, commit, uid, vec) in [
        (12, 40, 123, [0.824, 0.319, 0.111]),
        (11, 37, 1, [0.3, 0.5, 0.7]),
    ] {
        let entry = dbs.get(99, &key(&format!("{PRED}{VEC_ENTRY}"), 1)).unwrap();
        assert_eq!(bytes_to_uint64(&entry), Some(5));
        let edges = insert_at(&dbs, start, commit, uid, &vec, &mut rng).unwrap();
        let vkey = key(PRED, uid);
        assert_eq!(dbs.get(0, &vkey), dbs.get(99, &vkey));
        assert!(
            edge_names(&edges).contains(&format!("0-a__vector__{uid}")),
            "{:?}",
            edge_names(&edges)
        );
    }
}

#[test]
fn test_insert_helper_no_duplicate_neighbor_edges() {
    let mut rng = SimRng::new(3);
    let dbs = TsDbs::default();
    insert_at(&dbs, 0, 1, 5, &[0.1, 0.1, 0.1], &mut rng).unwrap();
    let edges2 = insert_at(&dbs, 12, 40, 123, &[0.824, 0.319, 0.111], &mut rng).unwrap();
    let edges3 = insert_at(&dbs, 50, 60, 456, &[0.5, 0.3, 0.2], &mut rng).unwrap();
    for edges in [&edges2, &edges3] {
        let mut counts: BTreeMap<u64, usize> = BTreeMap::new();
        edges
            .iter()
            .for_each(|e| *counts.entry(e.entity()).or_default() += 1);
        assert!(counts.values().all(|&c| c == 1), "{counts:?}");
    }
    assert!(
        edges3.len() >= 2,
        "the node and at least one neighbour: {edges3:?}"
    );
}

#[test]
fn test_the_first_insert_writes_empty_lists_at_every_level() {
    let txn = MapTxn::default();
    txn.db
        .borrow_mut()
        .insert(key(PRED, 9), float64_array_as_bytes(&[1.0, 2.0]));
    let index = flat_ph();
    let edges = index
        .insert(&TxnCache::new(&txn, 1), 9, &[1.0, 2.0], &mut || 0.5)
        .unwrap();
    assert_eq!(edges.len(), 2);
    assert_eq!(
        decode_uint64_matrix(edges[0].value()),
        Ok(vec![Vec::new(); 5])
    );
    assert_eq!(edges[1].value(), 9u64.to_be_bytes());
    // Inserting the entry point again writes nothing.
    assert_eq!(
        index.insert(&TxnCache::new(&txn, 1), 9, &[1.0, 2.0], &mut || 0.5),
        Ok(vec![])
    );
    // An entry point without its vector, in an index without another, is replaced by the node inserted.
    txn.db.borrow_mut().remove(&key(PRED, 9));
    let edges = flat_ph()
        .insert(&TxnCache::new(&txn, 1), 9, &[1.0, 2.0], &mut || 0.5)
        .unwrap();
    assert_eq!(
        edge_names(&edges),
        vec!["0-a__vector__9", "0-a__vector_entry_1"]
    );
}

/// A transaction whose reads of one key fail, as the store's failed read.
struct FailingTxn {
    inner: MapTxn,
    failing: Vec<u8>,
}

impl Txn for FailingTxn {
    type Error = HnswError;

    fn start_ts(&self) -> u64 {
        1
    }

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, HnswError> {
        if key == self.failing.as_slice() {
            return Err(PostingError::Store(StoreError::ReadFailed(
                key.to_vec(),
                "injected".into(),
            ))
            .into());
        }
        Txn::get(&self.inner, key)
    }

    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, HnswError> {
        Txn::get(self, key)
    }

    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, HnswError> {
        Txn::find(&self.inner, prefix, filter)
    }

    fn add_mutation(&self, key: &[u8], t: &KeyValue) -> Result<(), HnswError> {
        self.inner.add_mutation(key, t)
    }

    fn add_mutation_with_lock_held(&self, key: &[u8], t: &KeyValue) -> Result<(), HnswError> {
        self.inner.add_mutation(key, t)
    }

    fn lock_key(&self, _: &[u8]) {}

    fn unlock_key(&self, _: &[u8]) {}
}

fn is_read_failure(e: &HnswError) -> bool {
    matches!(
        &e.0,
        HnswErrorEnum::Posting(PostingError(PostingErrorEnum::Store(StoreError(
            StoreErrorEnum::ReadFailed { .. }
        ))))
    )
}

/// Deviation: Dgraph ignores the error of the entry point's read (`data, _ := txn.GetWithLockHeld(entryKey)`) and
/// makes the inserted node the entry point with empty lists, losing the graph; the port returns the failed read.
#[test]
fn test_a_failed_entry_read_is_an_error() {
    let entry_key = key(&format!("{PRED}{VEC_ENTRY}"), 1);
    let txn = FailingTxn {
        inner: MapTxn::default(),
        failing: entry_key,
    };
    let e = flat_ph()
        .insert(&TxnCache::new(&txn, 1), 9, &[1.0], &mut || 0.5)
        .unwrap_err();
    assert!(is_read_failure(&e), "{e}");
    assert!(txn.inner.db.borrow().is_empty(), "nothing written");
}

/// Deviation: Dgraph takes every failed vector read for an absent vector and skips the neighbour; the port returns
/// the failed read.
#[test]
fn test_a_failed_vector_read_is_an_error() {
    let inner = MapTxn::default();
    let index = flat_ph();
    for (uid, v) in [(3u64, [0.0, 1.0]), (6, [1.0, 1.0])] {
        inner
            .db
            .borrow_mut()
            .insert(key(PRED, uid), float64_array_as_bytes(&v));
        index
            .insert(&TxnCache::new(&inner, 1), uid, &v, &mut || 0.99)
            .unwrap();
    }
    let txn = FailingTxn {
        inner,
        failing: key(PRED, 6),
    };
    txn.inner
        .db
        .borrow_mut()
        .insert(key(PRED, 9), float64_array_as_bytes(&[2.0, 2.0]));
    let e = flat_ph()
        .insert(&TxnCache::new(&txn, 1), 9, &[2.0, 2.0], &mut || 0.99)
        .unwrap_err();
    assert!(is_read_failure(&e), "{e}");
}

/// What the decoders refuse: a neighbour matrix that does not fill its bytes, an entry point of other than 8 bytes,
/// a vector whose length is not a multiple of its float width, and a dead list that does not parse. A corruption that
/// leaves a valid encoding decodes and is not detected.
#[test]
fn test_corrupted_index_values_are_refused() {
    let lists = encode_uint64_matrix(&[vec![1, 2], vec![3]]);
    assert!(decode_uint64_matrix(&lists[..lists.len() - 1]).is_err());
    assert!(decode_uint64_matrix(&lists[..8]).is_err());
    assert!(decode_uint64_matrix(&[lists.clone(), vec![0; 8]].concat()).is_err());
    let mut many_rows = lists.clone();
    many_rows[0] = 200;
    assert!(decode_uint64_matrix(&many_rows).is_err());
    let mut long_row = lists.clone();
    long_row[8] = 9;
    assert!(decode_uint64_matrix(&long_row).is_err());
    let mut flipped = lists;
    flipped[16] ^= 0x40;
    assert_eq!(
        decode_uint64_matrix(&flipped),
        Ok(vec![vec![0x41, 2], vec![3]])
    );
    assert_eq!(bytes_to_uint64(&[0; 7]), None);
    assert_eq!(bytes_to_uint64(&[0; 9]), None);
    assert_eq!(bytes_as_float_array::<f32>(&[0; 6]), None);
    assert_eq!(bytes_as_float_array::<f64>(&[0; 12]), None);

    // Read through an insert, each is the store's corruption naming its key.
    for (attr, uid, value) in [
        (format!("{PRED}{VEC_ENTRY}"), 1, vec![1u8, 2, 3]),
        (PRED.to_string(), 3, vec![0u8; 12]),
    ] {
        let txn = MapTxn::default();
        let index = flat_ph();
        txn.db
            .borrow_mut()
            .insert(key(PRED, 3), float64_array_as_bytes(&[0.0, 1.0]));
        index
            .insert(&TxnCache::new(&txn, 1), 3, &[0.0, 1.0], &mut || 0.5)
            .unwrap();
        txn.db.borrow_mut().insert(key(&attr, uid), value);
        txn.db
            .borrow_mut()
            .insert(key(PRED, 6), float64_array_as_bytes(&[1.0, 1.0]));
        let e = flat_ph()
            .insert(&TxnCache::new(&txn, 1), 6, &[1.0, 1.0], &mut || 0.5)
            .unwrap_err();
        assert!(
            matches!(&e.0, HnswErrorEnum::Posting(PostingError(PostingErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted { key: k, .. }))))
                if *k == key(&attr, uid)),
            "{e}"
        );
    }
    let txn = MapTxn::default();
    txn.db
        .borrow_mut()
        .insert(key(PRED, 3), float64_array_as_bytes(&[0.0, 1.0]));
    flat_ph()
        .insert(&TxnCache::new(&txn, 1), 3, &[0.0, 1.0], &mut || 0.5)
        .unwrap();
    txn.db
        .borrow_mut()
        .insert(key(&format!("{PRED}{VEC_DEAD}"), 1), b"[x]".to_vec());
    txn.db
        .borrow_mut()
        .insert(key(PRED, 6), float64_array_as_bytes(&[1.0, 1.0]));
    let e = flat_ph()
        .insert(&TxnCache::new(&txn, 1), 6, &[1.0, 1.0], &mut || 0.5)
        .unwrap_err();
    assert_eq!(e.0, HnswErrorEnum::UintSlice("[x]".into()));
    let short = encode_uint64_matrix(&[vec![]]);
    txn.db
        .borrow_mut()
        .insert(key(&format!("{PRED}{VEC_KEYWORD}"), 3), short);
    txn.db
        .borrow_mut()
        .remove(&key(&format!("{PRED}{VEC_DEAD}"), 1));
    let e = flat_ph()
        .insert(&TxnCache::new(&txn, 1), 6, &[1.0, 1.0], &mut || 0.5)
        .unwrap_err();
    assert_eq!(
        e.0,
        HnswErrorEnum::LevelMissing {
            uid: 3,
            level: 1,
            levels: 1
        },
        "where Dgraph panics"
    );
}

/// Deviation: Dgraph's `DataKey` panics on a predicate without a namespace (`ParseNamespaceBytes` indexes the missing
/// part) and exits on an unparsable namespace (`log.Fatalf`); the port returns the key's error.
#[test]
fn test_a_predicate_without_a_namespace_is_a_key_error() {
    let txn = MapTxn::default();
    let o = opts(&[("maxLevels", "2")]);
    for name in ["plain", "zz-a"] {
        let e = PersistentHnsw::<f64>::new(name, &o, 64)
            .unwrap()
            .insert(&TxnCache::new(&txn, 1), 3, &[1.0], &mut || 0.5)
            .unwrap_err();
        assert!(matches!(e.0, HnswErrorEnum::Key(_)), "{name}: {e}");
    }
}
