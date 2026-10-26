/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_hnsw::{HnswError, TxnCache};
use ultragraphdb_traits_index::{CacheType, KeyValue, Txn};

use crate::utils::helpers::{InMemTxn, TsDbs};

#[test]
fn test_a_txn_cache_reads_through_its_transaction_at_its_timestamp() {
    let dbs = TsDbs::default();
    let txn = InMemTxn::new(&dbs, 3, 5);
    txn.add_mutation(b"k", &KeyValue::new(1, "a", b"v".to_vec()))
        .unwrap();
    let at_start = TxnCache::new(&txn, 3);
    assert_eq!(at_start.ts(), 3);
    assert_eq!(
        at_start.get(b"k"),
        Err(HnswError::NoValue()),
        "written at 5, read at 3"
    );
    assert_eq!(at_start.find(b"", |v| v == b"v"), Ok(0));
    let later = InMemTxn::new(&dbs, 7, 9);
    let tc = TxnCache::new(&later, 7);
    assert_eq!(tc.get(b"k"), Ok(b"v".to_vec()));
    assert_eq!(tc.find(b"", |v| v == b"v"), Ok(1));
    assert_eq!(tc.txn().start_ts(), 7);
}
