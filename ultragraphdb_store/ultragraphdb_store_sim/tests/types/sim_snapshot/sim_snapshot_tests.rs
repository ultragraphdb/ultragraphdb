/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

fn ns_key(ns: u64, tail: &[u8]) -> Vec<u8> {
    let mut key = vec![0x00];
    key.extend_from_slice(&ns.to_be_bytes());
    key.extend_from_slice(tail);
    key
}

#[test]
fn test_a_snapshot_reads_at_its_timestamp_with_the_ban_set_when_opened() {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let key = ns_key(3, b"a");
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(key.clone(), 2, 0x05, b"v".to_vec()));
    s.apply(batch, PersistMode::Buffer).unwrap();
    let before = s.snapshot(5).unwrap();
    s.ban_namespace(3).unwrap();
    let after = s.snapshot(5).unwrap();
    assert_eq!((before.read_ts(), before.id(), after.id()), (5, 0, 1));
    let item = before.get(&key).unwrap().expect("opened before the ban");
    assert_eq!(
        (item.key(), item.version(), item.meta(), item.value()),
        (&key[..], 2, 0x05, &b"v"[..])
    );
    assert_eq!(after.get(&key).unwrap(), None);
    assert_eq!(after.get_many(&[&key]).unwrap(), vec![None]);
    assert_eq!(
        after.scan(&Scan::prefix(vec![0x00])).count(),
        0,
        "a scan hides the banned keys too"
    );
}

#[test]
fn test_a_ban_covers_only_keys_longer_than_their_namespace() {
    // A key of exactly the prefix byte and eight namespace bytes names no namespace a ban covers.
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let short = ns_key(3, b"");
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(short.clone(), 1, 0, vec![]));
    s.apply(batch, PersistMode::Buffer).unwrap();
    s.ban_namespace(3).unwrap();
    assert_eq!(s.get_at(&short, 1).unwrap().unwrap().key(), &short[..]);
}

#[test]
fn test_get_many_answers_each_key_in_order() {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"a".to_vec(), 1, 0, b"1".to_vec()));
    batch.push(Entry::put(b"c".to_vec(), 2, 0, b"2".to_vec()));
    s.apply(batch, PersistMode::Buffer).unwrap();
    let got = s
        .snapshot(2)
        .unwrap()
        .get_many(&[b"c", b"b", b"a"])
        .unwrap();
    let values: Vec<Option<Vec<u8>>> = got
        .into_iter()
        .map(|i| i.map(|i| i.value().to_vec()))
        .collect();
    assert_eq!(values, [Some(b"2".to_vec()), None, Some(b"1".to_vec())]);
    let none: [&[u8]; 0] = [];
    assert_eq!(s.snapshot(2).unwrap().get_many(&none).unwrap(), vec![]);
}

#[test]
fn test_snapshot_numbers_count_up_across_recoveries() {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let first = s.snapshot(0).unwrap().id();
    s.crash();
    s.recover();
    let t = s.create_temporary().unwrap();
    let temporary = s.temporary_snapshot(&t).unwrap().id();
    let next = s.snapshot(0).unwrap().id();
    assert_eq!((first, temporary, next), (0, 1, 2));
}
