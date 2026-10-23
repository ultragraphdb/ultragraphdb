/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, StoreSnapshot, WriteBatch};

#[test]
fn test_a_snapshot_reads_at_its_timestamp_with_the_ban_set_when_opened() {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut key = vec![0x00];
    key.extend_from_slice(&3u64.to_be_bytes());
    key.push(b'a');
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(key.clone(), 2, 0, b"v".to_vec()));
    s.apply(batch, PersistMode::Buffer).unwrap();
    let before = s.snapshot(5).unwrap();
    s.ban_namespace(3).unwrap();
    let after = s.snapshot(5).unwrap();
    assert_eq!((before.read_ts(), before.id(), after.id()), (5, 0, 1));
    assert!(before.get(&key).unwrap().is_some());
    assert!(after.get(&key).unwrap().is_none());
}
