/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

#[test]
fn test_a_scan_yields_its_items_once_and_ends() {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"a".to_vec(), 1, 0, vec![]));
    batch.push(Entry::put(b"b".to_vec(), 1, 0, vec![]));
    s.apply(batch, PersistMode::Buffer).unwrap();
    let mut scan = s.snapshot(1).unwrap().scan(&Scan::prefix(Vec::new()));
    assert_eq!(scan.next().unwrap().unwrap().key(), b"a");
    assert_eq!(scan.next().unwrap().unwrap().key(), b"b");
    assert!(scan.next().is_none());
    assert!(scan.next().is_none());
}
