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

use std::collections::{BTreeMap, BTreeSet};

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
    InMemTxn, MapTxn, TsDbs, draw, edge_names, float64_array_as_bytes, key, opts, vkeys,
};

const PRED: &str = "0-a";

/// Dgraph's `flatPhs[0]`: 5 levels, an `efConstruction` of 16, an `efSearch` of 12, euclidean, float64.
fn flat_ph() -> PersistentHnsw<f64> {
    let o = opts(&[
        ("maxLevels", "5"),
        ("efConstruction", "16"),
        ("efSearch", "12"),
    ]);
    PersistentHnsw::new(vkeys(PRED), &o, 64).unwrap()
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
        assert!(
            !edge_names(&edges).contains(&"0-a__vector_entry_1".to_string()),
            "a live entry point is not rewritten"
        );
    }
    let entry = dbs.get(99, &key(&format!("{PRED}{VEC_ENTRY}"), 1)).unwrap();
    assert_eq!(bytes_to_uint64(&entry), Some(5));
}

#[test]
fn test_insert_helper_no_duplicate_neighbor_edges() {
    let mut rng = SimRng::new(3);
    let dbs = TsDbs::default();
    insert_at(&dbs, 0, 1, 5, &[0.1, 0.1, 0.1], &mut rng).unwrap();
    let edges2 = insert_at(&dbs, 12, 40, 123, &[0.824, 0.319, 0.111], &mut rng).unwrap();
    let edges3 = insert_at(&dbs, 50, 60, 456, &[0.5, 0.3, 0.2], &mut rng).unwrap();
    for (uid, edges) in [(123, &edges2), (456, &edges3)] {
        let mut counts: BTreeMap<u64, usize> = BTreeMap::new();
        edges
            .iter()
            .for_each(|e| *counts.entry(e.entity()).or_default() += 1);
        assert!(counts.values().all(|&c| c == 1), "{counts:?}");
        // The invariant of addNeighbors: the insert writes the node's lists and the lists of exactly the neighbours
        // they name, each gaining the node as an inbound edge.
        let own = edges
            .iter()
            .find(|e| e.entity() == uid)
            .expect("the node's lists");
        let named: BTreeSet<u64> = decode_uint64_matrix(own.value())
            .unwrap()
            .into_iter()
            .flatten()
            .collect();
        let written: BTreeSet<u64> = counts.keys().copied().filter(|&u| u != uid).collect();
        assert_eq!(written, named, "insert of {uid}");
        assert!(!named.is_empty(), "insert of {uid} links to the graph");
        for e in edges.iter().filter(|e| e.entity() != uid) {
            assert!(
                decode_uint64_matrix(e.value())
                    .unwrap()
                    .iter()
                    .flatten()
                    .any(|&u| u == uid),
                "{} gains {uid}",
                e.entity()
            );
        }
    }
    assert_eq!(
        edges2.iter().map(|e| e.entity()).collect::<BTreeSet<_>>(),
        BTreeSet::from([5, 123]),
        "the second insert links to the only other node"
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
    assert_eq!(
        decode_uint64_matrix(&lists[..lists.len() - 1]),
        Err("a neighbour matrix whose length is not a multiple of 8 bytes")
    );
    assert_eq!(
        decode_uint64_matrix(&lists[..8]),
        Err("a neighbour matrix with more rows than its bytes hold")
    );
    // Two rows, the first of 2 uids, and one uid held.
    assert_eq!(
        decode_uint64_matrix(&lists[..24]),
        Err("a neighbour row longer than the matrix holds")
    );
    assert_eq!(
        decode_uint64_matrix(&[lists.clone(), vec![0; 8]].concat()),
        Err("bytes after a neighbour matrix's last row")
    );
    let mut many_rows = lists.clone();
    many_rows[0] = 200;
    assert_eq!(
        decode_uint64_matrix(&many_rows),
        Err("a neighbour matrix with more rows than its bytes hold")
    );
    let mut long_row = lists.clone();
    long_row[8] = 9;
    assert_eq!(
        decode_uint64_matrix(&long_row),
        Err("a neighbour row longer than the matrix holds")
    );
    // Two rows named, the first [7] filling the bytes: the second row's length is missing.
    let mut rows_unheld = encode_uint64_matrix(&[vec![7]]);
    rows_unheld[0] = 2;
    assert_eq!(
        decode_uint64_matrix(&rows_unheld),
        Err("a neighbour matrix shorter than its rows")
    );
    assert_eq!(decode_uint64_matrix(&[]), Ok(vec![]));
    assert_eq!(decode_uint64_matrix(&[0; 8]), Ok(vec![]), "no rows");
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

/// Deviation (ledger 45): when the entry point has lost its vector, Dgraph makes the first node in key order holding
/// one the entry point by rewriting it with empty neighbour lists at every level, wiping its edges, and returns
/// without inserting the new vector (`helper.go:419-428`, `persistent_hnsw.go:577-580`). The port moves the entry
/// point to that node, keeps its lists, and inserts the new vector from it.
#[test]
fn test_a_dead_entry_point_hands_over_and_the_insert_goes_ahead() {
    let txn = MapTxn::default();
    let index = || flat_ph();
    for (uid, v) in [(3u64, [0.0, 0.0]), (6, [1.0, 0.0]), (9, [2.0, 0.0])] {
        txn.db
            .borrow_mut()
            .insert(key(PRED, uid), float64_array_as_bytes(&v));
        index()
            .insert(&TxnCache::new(&txn, 1), uid, &v, &mut || 0.99)
            .unwrap();
    }
    let lists_of = |uid: u64| {
        decode_uint64_matrix(&txn.db.borrow()[&key(&format!("{PRED}{VEC_KEYWORD}"), uid)]).unwrap()
    };
    let six_before = lists_of(6);
    assert!(six_before.iter().any(|l| !l.is_empty()), "{six_before:?}");
    // The entry point, 3, loses its vector.
    txn.db.borrow_mut().remove(&key(PRED, 3));
    txn.db
        .borrow_mut()
        .insert(key(PRED, 12), float64_array_as_bytes(&[1.5, 0.0]));
    let edges = index()
        .insert(&TxnCache::new(&txn, 1), 12, &[1.5, 0.0], &mut || 0.99)
        .unwrap();
    let entry = txn.db.borrow()[&key(&format!("{PRED}{VEC_ENTRY}"), 1)].clone();
    assert_eq!(bytes_to_uint64(&entry), Some(6));
    let names = edge_names(&edges);
    assert!(
        names.contains(&"0-a__vector_entry_1".to_string()),
        "{names:?}"
    );
    assert!(names.contains(&"0-a__vector__12".to_string()), "{names:?}");
    assert!(
        lists_of(12).iter().any(|l| !l.is_empty()),
        "12 was inserted"
    );
    let six_after = lists_of(6);
    assert!(
        six_before
            .iter()
            .zip(&six_after)
            .all(|(before, after)| before
                .iter()
                .filter(|&&u| u != 3)
                .all(|u| after.contains(u))),
        "6 keeps its edges: {six_before:?} -> {six_after:?}"
    );
}

/// One level, an `efConstruction` of 16 and an `efSearch` of 12: every node on the one level whatever the draw.
fn one_level() -> PersistentHnsw<f64> {
    let o = opts(&[
        ("maxLevels", "1"),
        ("efConstruction", "16"),
        ("efSearch", "12"),
    ]);
    PersistentHnsw::new(vkeys(PRED), &o, 64).unwrap()
}

fn insert_one(txn: &MapTxn, uid: u64, v: &[f64]) -> Vec<KeyValue> {
    txn.db
        .borrow_mut()
        .insert(key(PRED, uid), float64_array_as_bytes(v));
    one_level()
        .insert(&TxnCache::new(txn, 1), uid, v, &mut || 0.5)
        .unwrap()
}

fn lists(edges: &[KeyValue]) -> BTreeMap<u64, Vec<Vec<u64>>> {
    edges
        .iter()
        .filter(|e| e.attr().ends_with(VEC_KEYWORD))
        .map(|e| (e.entity(), decode_uint64_matrix(e.value()).unwrap()))
        .collect()
}

/// By hand, on a line: 3 at 0, 6 at 1, 9 at 2. After 6 is deleted, 12 at 1.5 is searched from 3, past 6, which has
/// no vector, to 9 (0.5 away) and 3 (1.5 away); every list it writes leaves the dead 6 out and is sorted nearest first.
#[test]
fn test_an_insert_leaves_dead_nodes_out_of_the_lists_it_writes() {
    let txn = MapTxn::default();
    insert_one(&txn, 3, &[0.0, 0.0]);
    insert_one(&txn, 6, &[1.0, 0.0]);
    let written = lists(&insert_one(&txn, 9, &[2.0, 0.0]));
    assert_eq!(
        written,
        BTreeMap::from([
            (3, vec![vec![6, 9]]),
            (6, vec![vec![3, 9]]),
            (9, vec![vec![6, 3]])
        ])
    );
    txn.db.borrow_mut().remove(&key(PRED, 6));
    txn.db
        .borrow_mut()
        .insert(key(&format!("{PRED}{VEC_DEAD}"), 1), b"[6]".to_vec());
    let written = lists(&insert_one(&txn, 12, &[1.5, 0.0]));
    assert_eq!(
        written,
        BTreeMap::from([
            (3, vec![vec![12, 9]]),
            (9, vec![vec![12, 3]]),
            (12, vec![vec![9, 3]]),
        ])
    );
}

/// A node never scores against itself: a list naming its own node, as a corrupted one may, takes the metric's worst
/// score, as Dgraph's `uidScoreForNode` gives it, so a new neighbour goes before it. By hand: 6's list is [6, 3], and
/// 9 at 1.1, 0.1 from 6, beats both, where a score of 0 for 6 itself would put 9 second.
#[test]
fn test_a_node_scores_worst_against_itself() {
    let txn = MapTxn::default();
    insert_one(&txn, 3, &[0.0, 0.0]);
    insert_one(&txn, 6, &[1.0, 0.0]);
    txn.db.borrow_mut().insert(
        key(&format!("{PRED}{VEC_KEYWORD}"), 6),
        encode_uint64_matrix(&[vec![6, 3]]),
    );
    let written = lists(&insert_one(&txn, 9, &[1.1, 0.0]));
    assert_eq!(written[&6], vec![vec![9, 6, 3]]);
}
