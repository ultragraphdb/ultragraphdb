/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{Contents, FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

#[test]
fn test_empty_contents_hold_nothing() {
    let contents = Contents::default();
    assert!(contents.keys().is_empty());
    assert_eq!(
        (
            contents.applied_ts(),
            contents.discard_ts(),
            contents.tombstone_horizon()
        ),
        (0, 0, None)
    );
    assert!(contents.banned().is_empty());
}

#[test]
fn test_contents_compare_equal_exactly_when_the_stores_hold_the_same() {
    let stores: Vec<SimStore> = (0..2)
        .map(|_| SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new()))
        .collect();
    for s in &stores {
        let mut batch = WriteBatch::new();
        batch.push(Entry::put(b"k".to_vec(), 1, 0, b"v".to_vec()));
        s.apply(batch, PersistMode::Buffer).unwrap();
    }
    assert_eq!(stores[0].contents(), stores[1].contents());
    stores[1].ban_namespace(3).unwrap();
    assert_ne!(stores[0].contents(), stores[1].contents());
}
