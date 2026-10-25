/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_sim::{Fault, FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

fn store_of(keys: &[&[u8]]) -> SimStore {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut batch = WriteBatch::new();
    keys.iter()
        .for_each(|k| batch.push(Entry::put(k.to_vec(), 1, 0, k.to_vec())));
    s.apply(batch, PersistMode::Buffer).unwrap();
    s
}

#[test]
fn test_a_scan_yields_its_items_once_and_ends() {
    let s = store_of(&[b"a", b"b"]);
    let mut scan = s.snapshot(1).unwrap().scan(&Scan::prefix(Vec::new()));
    let a = scan.next().unwrap().unwrap();
    assert_eq!((a.key(), a.version(), a.value()), (&b"a"[..], 1, &b"a"[..]));
    assert_eq!(scan.next().unwrap().unwrap().key(), b"b");
    assert!(scan.next().is_none());
    assert!(scan.next().is_none());
}

#[test]
fn test_a_scan_of_nothing_ends_at_once() {
    let s = store_of(&[b"a"]);
    let mut scan = s.snapshot(1).unwrap().scan(&Scan::prefix(b"z".to_vec()));
    assert!(scan.next().is_none());
}

#[test]
fn test_a_failed_scan_yields_its_error_last() {
    let s = store_of(&[b"a", b"b", b"c"]);
    s.inject(Fault::FailNextScanAfter(1));
    let items: Vec<_> = s
        .snapshot(1)
        .unwrap()
        .scan(&Scan::prefix(Vec::new()))
        .collect();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].as_ref().unwrap().key(), b"a");
    assert_eq!(
        items[1],
        Err(StoreError::ReadFailed(
            b"b".to_vec(),
            "simulated read failure partway through a scan".into()
        ))
    );
}
