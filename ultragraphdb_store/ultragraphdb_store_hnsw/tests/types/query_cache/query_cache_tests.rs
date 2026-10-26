/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_hnsw::{HnswError, QueryCache};
use ultragraphdb_traits_index::CacheType;

use crate::utils::helpers::{InMemLocalCache, TsDbs};

#[test]
fn test_a_query_cache_reads_through_its_local_cache() {
    let dbs = TsDbs::default();
    dbs.write_from(4, b"k", b"v");
    let early = InMemLocalCache::new(&dbs, 3);
    assert_eq!(
        QueryCache::new(&early, 3).get(b"k"),
        Err(HnswError::NoValue())
    );
    assert_eq!(QueryCache::new(&early, 3).ts(), 3);
    let late = InMemLocalCache::new(&dbs, 4);
    let qc = QueryCache::new(&late, 4);
    assert_eq!((qc.get(b"k"), qc.ts()), (Ok(b"v".to_vec()), 4));
    // Dgraph's double: 1 when a value passes, 0 when none does.
    assert_eq!(qc.find(b"", |v| v == b"x"), Ok(0));
    assert_eq!(qc.find(b"", |v| v == b"v"), Ok(1));
}
