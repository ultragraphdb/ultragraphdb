/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store's contract (the kv-store specification) as the simulated store keeps it.

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

fn store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

fn write(store: &SimStore, entries: Vec<Entry>) {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store
        .apply(batch, PersistMode::Buffer)
        .expect("no fault is configured");
}

fn put(key: &[u8], version: u64, value: &[u8]) -> Entry {
    Entry::put(key.to_vec(), version, 0x04, value.to_vec())
}

/// A key of namespace `ns`.
fn ns_key(ns: u64, tail: &[u8]) -> Vec<u8> {
    let mut key = vec![0x00];
    key.extend_from_slice(&ns.to_be_bytes());
    key.extend_from_slice(tail);
    key
}

fn versions(store: &SimStore, key: &[u8], ts: u64) -> Vec<u64> {
    let snapshot = store.snapshot(ts).expect("readable");
    snapshot
        .versions(key)
        .map(|i| i.expect("no fault").version())
        .collect()
}

fn scanned(store: &SimStore, scan: &Scan, ts: u64) -> Vec<(Vec<u8>, u64)> {
    let snapshot = store.snapshot(ts).expect("readable");
    snapshot
        .scan(scan)
        .map(|i| i.expect("no fault"))
        .map(|i| (i.key().to_vec(), i.version()))
        .collect()
}

#[test]
fn test_a_keys_versions_iterate_newest_first_with_their_meta() {
    let s = store();
    for v in [3, 7, 5] {
        write(&s, vec![Entry::put(b"k".to_vec(), v, v as u8, vec![])]);
    }
    let snapshot = s.snapshot(10).unwrap();
    let items: Vec<(u64, u8)> = snapshot
        .versions(b"k")
        .map(|i| i.unwrap())
        .map(|i| (i.version(), i.meta()))
        .collect();
    assert_eq!(items, vec![(7, 7), (5, 5), (3, 3)]);
}

#[test]
fn test_a_version_below_the_newest_can_be_written() {
    let s = store();
    write(&s, vec![put(b"k", 9, b"a")]);
    write(&s, vec![put(b"k", 6, b"b")]);
    assert_eq!(versions(&s, b"k", 10), vec![9, 6]);
}

#[test]
fn test_rewriting_a_version_replaces_it() {
    let s = store();
    write(&s, vec![put(b"k", 4, b"first")]);
    write(&s, vec![put(b"k", 4, b"second")]);
    assert_eq!(s.get_at(b"k", 4).unwrap().unwrap().value(), b"second");
    assert_eq!(versions(&s, b"k", 4), vec![4]);
}

#[test]
fn test_a_key_that_prefixes_another_sorts_first() {
    let s = store();
    for v in [3, 5] {
        write(&s, vec![put(b"\x02ab", v, b""), put(b"\x02abc", v, b"")]);
    }
    let scan = Scan::prefix(b"\x02a".to_vec()).all_versions();
    let keys = scanned(&s, &scan, 9);
    let expected: Vec<(Vec<u8>, u64)> = vec![
        (b"\x02ab".to_vec(), 5),
        (b"\x02ab".to_vec(), 3),
        (b"\x02abc".to_vec(), 5),
        (b"\x02abc".to_vec(), 3),
    ];
    assert_eq!(keys, expected);
}

#[test]
fn test_an_oversized_key_refuses_its_whole_batch() {
    let s = store();
    let mut batch = WriteBatch::new();
    batch.push(put(b"fine", 1, b""));
    batch.push(put(&vec![1; 65_001], 1, b""));
    let err = s.apply(batch, PersistMode::Buffer).unwrap_err();
    assert!(
        err.to_string()
            .starts_with("Key with size 65001 exceeded 65000 limit")
    );
    assert_eq!(s.get_at(b"fine", 1).unwrap(), None);

    let mut batch = WriteBatch::new();
    batch.push(put(b"fine", 1, b""));
    batch.push(put(&vec![0; 40_000], 1, b""));
    let err = s.apply(batch, PersistMode::Buffer).unwrap_err();
    assert!(
        err.to_string()
            .starts_with("Key with size 80010 exceeded 65535 limit")
    );
    assert_eq!(s.get_at(b"fine", 1).unwrap(), None);
    assert!(
        !s.needs_recovery(),
        "a refused batch does not poison the store"
    );
}

#[test]
fn test_a_read_takes_the_newest_version_at_or_below_its_timestamp() {
    let s = store();
    write(&s, vec![put(b"k", 3, b"three"), put(b"k", 7, b"seven")]);
    assert_eq!(s.get_at(b"k", 5).unwrap().unwrap().version(), 3);
    assert_eq!(s.get_at(b"k", 7).unwrap().unwrap().value(), b"seven");
    assert_eq!(s.get_at(b"k", 2).unwrap(), None);
}

#[test]
fn test_a_deletion_hides_the_key() {
    let s = store();
    write(
        &s,
        vec![put(b"k", 3, b"v"), Entry::deletion(b"k".to_vec(), 6)],
    );
    assert_eq!(s.get_at(b"k", 7).unwrap(), None);
    assert_eq!(s.get_at(b"k", 4).unwrap().unwrap().version(), 3);
    assert_eq!(versions(&s, b"k", 7), Vec::<u64>::new());
    assert!(scanned(&s, &Scan::prefix(b"k".to_vec()), 7).is_empty());
}

#[test]
fn test_a_read_below_the_discard_timestamp_is_refused() {
    let s = store();
    write(&s, vec![put(b"k", 3, b"v")]);
    s.set_discard_ts(10).unwrap();
    assert_eq!(
        s.get_at(b"k", 5).unwrap_err().to_string(),
        "Transaction is too old"
    );
    assert!(s.get_at(b"k", 10).is_ok());
}

#[test]
fn test_a_scan_is_unaffected_by_a_later_batch() {
    let s = store();
    write(&s, vec![put(b"p1", 1, b"")]);
    let snapshot = s.snapshot(100).unwrap();
    write(&s, vec![put(b"p2", 2, b""), put(b"p1", 3, b"")]);
    let seen: Vec<Item> = snapshot
        .scan(&Scan::prefix(b"p".to_vec()))
        .map(Result::unwrap)
        .collect();
    assert_eq!(seen.len(), 1);
    assert_eq!((seen[0].key(), seen[0].version()), (&b"p1"[..], 1));
}

#[test]
fn test_a_scan_reports_each_key_once_with_its_newest_version() {
    let s = store();
    for v in 1..=4 {
        write(
            &s,
            vec![
                put(b"pa", v, b""),
                put(b"pb", v + 10, b""),
                put(b"pc", v * 2, b""),
            ],
        );
    }
    let got = scanned(&s, &Scan::prefix(b"p".to_vec()), 6);
    assert_eq!(got, vec![(b"pa".to_vec(), 4), (b"pc".to_vec(), 6)]);
    let all = scanned(&s, &Scan::prefix(b"p".to_vec()).all_versions(), 6);
    assert_eq!(
        all.iter()
            .filter(|(k, _)| k == b"pa")
            .map(|(_, v)| *v)
            .collect::<Vec<_>>(),
        vec![4, 3, 2, 1]
    );
    assert_eq!(
        all.iter()
            .filter(|(k, _)| k == b"pc")
            .map(|(_, v)| *v)
            .collect::<Vec<_>>(),
        vec![6, 4, 2]
    );
}

#[test]
fn test_a_reverse_scan_descends_with_versions_newest_first() {
    let s = store();
    write(
        &s,
        vec![put(b"a", 1, b""), put(b"b", 1, b""), put(b"c", 1, b"")],
    );
    write(&s, vec![put(b"b", 2, b"")]);
    let all = scanned(&s, &Scan::prefix(Vec::new()).reverse().all_versions(), 5);
    let expected: Vec<(Vec<u8>, u64)> = vec![
        (b"c".to_vec(), 1),
        (b"b".to_vec(), 2),
        (b"b".to_vec(), 1),
        (b"a".to_vec(), 1),
    ];
    assert_eq!(all, expected);
    let sought = scanned(
        &s,
        &Scan::prefix(Vec::new()).reverse().seek(b"b".to_vec()),
        5,
    );
    assert_eq!(sought, vec![(b"b".to_vec(), 2), (b"a".to_vec(), 1)]);
    let forward = scanned(&s, &Scan::prefix(Vec::new()).seek(b"b".to_vec()), 5);
    assert_eq!(forward, vec![(b"b".to_vec(), 2), (b"c".to_vec(), 1)]);
}

#[test]
fn test_a_batch_holds_entries_at_several_versions() {
    let s = store();
    write(&s, vec![put(b"a", 12, b""), put(b"b", 13, b"")]);
    let at12 = s.snapshot(12).unwrap().get_many(&[b"a", b"b"]).unwrap();
    assert_eq!(
        at12.iter().map(Option::is_some).collect::<Vec<_>>(),
        vec![true, false]
    );
    let at13 = s.snapshot(13).unwrap().get_many(&[b"a", b"b"]).unwrap();
    assert!(at13.iter().all(Option::is_some));
}

#[test]
fn test_a_batch_records_the_highest_applied_timestamp() {
    let s = store();
    let mut batch = WriteBatch::new();
    batch.set_applied_ts(9);
    s.apply(batch, PersistMode::Buffer).unwrap();
    let mut rollup = WriteBatch::new();
    rollup.push(put(b"k", 4, b""));
    s.apply(rollup, PersistMode::Buffer).unwrap();
    let mut older = WriteBatch::new();
    older.set_applied_ts(5);
    s.apply(older, PersistMode::Buffer).unwrap();
    assert_eq!(s.applied_ts(), 9);
}

#[test]
fn test_a_dropped_prefix_is_gone_after_reopening_and_below_the_drop() {
    let s = store();
    write(
        &s,
        vec![
            put(b"\x00a1", 3, b""),
            put(b"\x00a2", 3, b""),
            put(b"\x00b1", 3, b""),
        ],
    );
    s.drop_prefixes(8, &[b"\x00a".to_vec()], vec![put(b"\x00a9", 1, b"initial")])
        .unwrap();
    s.crash();
    s.recover();
    assert_eq!(
        s.get_at(b"\x00a1", 5).unwrap(),
        None,
        "a read below the drop finds nothing of it"
    );
    assert_eq!(s.get_at(b"\x00a2", 9).unwrap(), None);
    assert_eq!(s.get_at(b"\x00b1", 9).unwrap().unwrap().version(), 3);
    assert_eq!(s.get_at(b"\x00a9", 9).unwrap().unwrap().value(), b"initial");
}

#[test]
fn test_dropping_everything_keeps_only_its_entries_and_clears_the_metadata() {
    let s = store();
    let mut batch = WriteBatch::new();
    batch.push(put(b"old", 3, b""));
    batch.set_applied_ts(3);
    s.apply(batch, PersistMode::Buffer).unwrap();
    s.set_discard_ts(2).unwrap();
    s.set_tombstone_horizon(1).unwrap();
    s.drop_all(vec![put(b"\x01schema", 1, b"s")]).unwrap();
    s.power_loss();
    s.recover();
    let contents = s.contents();
    assert_eq!(
        contents.keys().keys().collect::<Vec<_>>(),
        vec![&b"\x01schema".to_vec()]
    );
    assert_eq!(
        (s.applied_ts(), s.discard_ts(), contents.tombstone_horizon()),
        (0, 0, None)
    );
}

#[test]
fn test_a_banned_namespace_ignores_writes_and_hides_its_keys() {
    let s = store();
    let (five, six) = (ns_key(5, b"\x00\x01a"), ns_key(6, b"\x00\x01a"));
    write(&s, vec![put(&five, 1, b"before")]);
    s.ban_namespace(5).unwrap();
    write(&s, vec![put(&five, 2, b""), put(&six, 2, b"")]);
    assert_eq!(s.get_at(&five, 3).unwrap(), None);
    assert!(s.get_at(&six, 3).unwrap().is_some());
    assert!(
        scanned(&s, &Scan::prefix(vec![0x00]), 3)
            .iter()
            .all(|(k, _)| *k != five)
    );
    assert_eq!(
        s.contents().versions(&five).len(),
        1,
        "the ban hides the earlier version, it does not remove it"
    );
}

#[test]
fn test_dropping_everything_clears_the_banned_set() {
    let s = store();
    let five = ns_key(5, b"\x00\x01a");
    s.ban_namespace(5).unwrap();
    s.drop_all(Vec::new()).unwrap();
    write(&s, vec![put(&five, 1, b"")]);
    assert!(s.get_at(&five, 1).unwrap().is_some());
}

#[test]
fn test_a_poisoned_store_refuses_every_write_until_recovered() {
    let s = store();
    s.inject(ultragraphdb_store_sim::Fault::FailNextBatch);
    let failed = s.apply(WriteBatch::new(), PersistMode::Buffer);
    assert!(matches!(
        failed,
        Err(StoreError(StoreErrorEnum::WriteFailed(_)))
    ));
    let poisoned = Err(StoreError::Poisoned());
    assert_eq!(s.apply(WriteBatch::new(), PersistMode::Buffer), poisoned);
    assert_eq!(s.sync(), poisoned);
    assert_eq!(s.compact(), poisoned);
    assert_eq!(s.sweep_deletions(), poisoned);
    assert_eq!(s.ban_namespace(1), poisoned);
    assert_eq!(s.set_discard_ts(1), poisoned);
    assert_eq!(s.set_tombstone_horizon(1), poisoned);
    assert_eq!(s.drop_all(Vec::new()), poisoned);
    assert_eq!(s.drop_prefixes(1, &[], Vec::new()), poisoned);
    assert!(s.snapshot(0).is_ok(), "a poisoned process still reads");
    s.recover();
    assert!(s.apply(WriteBatch::new(), PersistMode::Buffer).is_ok());
}
