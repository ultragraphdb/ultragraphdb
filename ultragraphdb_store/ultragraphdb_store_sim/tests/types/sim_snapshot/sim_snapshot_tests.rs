/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_sim::{Corruption, FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

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

fn truncated(key: &[u8]) -> StoreError {
    StoreError::Corrupted(
        key.to_vec(),
        "value too short to hold the version prefix".to_string(),
    )
}

#[test]
fn test_a_snapshot_opened_before_a_drop_reads_the_corrupted_version_the_drop_removed() {
    // The history of ultragraphdb_store_kv's test_an_open_snapshot_reads_the_undecodable_version_a_later_drop_removes,
    // with the same answer: the compaction keeps the undecodable version, and the drop removes it from what later
    // snapshots read only.
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let key = b"pk".to_vec();
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(key.clone(), 1, 0x04, b"old".to_vec()));
    batch.push(Entry::put(key.clone(), 2, 0x08, b"rolled".to_vec()).with_discard());
    s.apply(batch, PersistMode::Buffer).unwrap();
    s.corrupt(&key, 1, Corruption::Truncate);
    s.set_discard_ts(2).unwrap();
    s.compact().unwrap();
    let reader = s.snapshot(2).unwrap();
    s.drop_prefixes(3, &[b"p".to_vec()], Vec::new()).unwrap();
    assert_eq!(
        reader.versions(&key).collect::<Vec<_>>(),
        vec![
            Ok(Item::new(key.clone(), 2, 0x08, true, b"rolled".to_vec())),
            Err(truncated(&key)),
        ]
    );
    assert_eq!(s.snapshot(2).unwrap().versions(&key).count(), 0);
}

#[test]
fn test_a_rewrite_or_a_drop_of_everything_heals_only_the_snapshots_opened_after_it() {
    let s = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let write = || {
        let mut batch = WriteBatch::new();
        batch.push(Entry::put(b"k".to_vec(), 1, 0x04, b"v".to_vec()));
        s.apply(batch, PersistMode::Buffer).unwrap();
    };
    let item = Some(Item::new(b"k".to_vec(), 1, 0x04, false, b"v".to_vec()));
    write();
    let before = s.snapshot(1).unwrap();
    s.corrupt(b"k", 1, Corruption::Truncate);
    assert_eq!(
        before.get(b"k"),
        Err(truncated(b"k")),
        "a corruption after the snapshot opened, of a version it reads"
    );
    write();
    let rewritten = s.snapshot(1).unwrap();
    s.drop_all(Vec::new()).unwrap();
    let dropped = s.snapshot(1).unwrap();
    assert_eq!(before.get(b"k"), Err(truncated(b"k")));
    assert_eq!(before.get_many(&[b"k"]), Err(truncated(b"k")));
    assert_eq!(rewritten.get(b"k"), Ok(item));
    assert_eq!(dropped.get(b"k"), Ok(None));
}
