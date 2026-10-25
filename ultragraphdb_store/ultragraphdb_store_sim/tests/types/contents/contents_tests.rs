/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_store_sim::{Contents, FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, RebuildRecord, Store, WriteBatch};

#[test]
fn test_empty_contents_hold_nothing() {
    for contents in [
        Contents::default(),
        SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new()).contents(),
    ] {
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
        assert!(contents.rebuild_records().is_empty());
        assert!(contents.versions(b"k").is_empty());
    }
}

#[test]
fn test_contents_list_keys_in_order_with_their_rebuild_records() {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut batch = WriteBatch::new();
    for key in [&b"b"[..], b"a", b"ab"] {
        batch.push(Entry::put(key.to_vec(), 1, 0, vec![]));
    }
    s.apply(batch, PersistMode::Buffer).unwrap();
    s.ban_namespace(9).unwrap();
    s.ban_namespace(3).unwrap();
    s.write_rebuild_record(&RebuildRecord::new(4, vec![b"\x02p".to_vec()]))
        .unwrap();
    let contents = s.contents();
    assert_eq!(
        contents.keys().keys().cloned().collect::<Vec<_>>(),
        [b"a".to_vec(), b"ab".to_vec(), b"b".to_vec()]
    );
    assert_eq!(contents.banned(), &BTreeSet::from([3, 9]));
    assert_eq!(
        contents.rebuild_records().iter().collect::<Vec<_>>(),
        [(&4, &vec![b"\x02p".to_vec()])]
    );
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
    stores[0].ban_namespace(3).unwrap();
    assert_eq!(stores[0].contents(), stores[1].contents());
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"k".to_vec(), 1, 0, b"w".to_vec()));
    stores[1].apply(batch, PersistMode::Buffer).unwrap();
    assert_ne!(
        stores[0].contents(),
        stores[1].contents(),
        "the same version with another value differs"
    );
}
