/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store's contract (the kv-store specification) as the fjall store keeps it: the simulated store's contract
//! tests, run on fjall, with a crash as a close and a reopen.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

use crate::test_store::{TestDir, ns_key, put, scanned, versions, write};

#[test]
fn test_a_keys_versions_iterate_newest_first_with_their_meta() {
    let dir = TestDir::new("versions");
    let s = dir.open();
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
    let dir = TestDir::new("below");
    let s = dir.open();
    write(&s, vec![put(b"k", 9, b"a")]);
    write(&s, vec![put(b"k", 6, b"b")]);
    assert_eq!(versions(&s, b"k", 10), vec![9, 6]);
}

#[test]
fn test_rewriting_a_version_replaces_it_across_batches_and_within_one() {
    let dir = TestDir::new("rewrite");
    let s = dir.open();
    write(&s, vec![put(b"k", 4, b"first")]);
    write(&s, vec![put(b"k", 4, b"second")]);
    assert_eq!(s.get_at(b"k", 4).unwrap().unwrap().value(), b"second");
    assert_eq!(versions(&s, b"k", 4), vec![4]);
    write(&s, vec![put(b"j", 4, b"first"), put(b"j", 4, b"later")]);
    assert_eq!(s.get_at(b"j", 4).unwrap().unwrap().value(), b"later");
    drop(s);
    let s = dir.open();
    assert_eq!(
        s.get_at(b"j", 4).unwrap().unwrap().value(),
        b"later",
        "the journal replays the later one"
    );
    assert_eq!(s.get_at(b"k", 4).unwrap().unwrap().value(), b"second");
}

#[test]
fn test_a_key_that_prefixes_another_sorts_first() {
    let dir = TestDir::new("prefix-order");
    let s = dir.open();
    for v in [3, 5] {
        write(
            &s,
            vec![
                put(b"\x02ab", v, b""),
                put(b"\x02abc", v, b""),
                put(b"\x02a\x00b", v, b""),
            ],
        );
    }
    let scan = Scan::prefix(b"\x02a".to_vec()).all_versions();
    let expected: Vec<(Vec<u8>, u64)> = vec![
        (b"\x02a\x00b".to_vec(), 5),
        (b"\x02a\x00b".to_vec(), 3),
        (b"\x02ab".to_vec(), 5),
        (b"\x02ab".to_vec(), 3),
        (b"\x02abc".to_vec(), 5),
        (b"\x02abc".to_vec(), 3),
    ];
    assert_eq!(scanned(&s, &scan, 9), expected);
    let mut reversed = expected.clone();
    reversed.reverse();
    let reverse: Vec<(Vec<u8>, u64)> = scanned(&s, &scan.clone().reverse(), 9);
    let mut keys: Vec<Vec<u8>> = reverse.iter().map(|(k, _)| k.clone()).collect();
    keys.dedup();
    assert_eq!(
        keys,
        vec![
            b"\x02abc".to_vec(),
            b"\x02ab".to_vec(),
            b"\x02a\x00b".to_vec()
        ]
    );
    assert!(
        reverse
            .chunks(2)
            .all(|pair| pair[0].1 == 5 && pair[1].1 == 3)
    );
    assert_eq!(
        scanned(&s, &Scan::prefix(b"\x02ab".to_vec()), 9),
        vec![(b"\x02ab".to_vec(), 5), (b"\x02abc".to_vec(), 5)],
        "ab and abc, not a\\0b"
    );
}

#[test]
fn test_an_oversized_key_refuses_its_whole_batch() {
    let dir = TestDir::new("oversized");
    let s = dir.open();
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
        !s.is_poisoned(),
        "a refused batch does not poison the store"
    );

    // The longest user key, and the longest stored key fjall's journal can hold, survive a reopen intact.
    let longest = vec![7u8; 65_000];
    let zeros = vec![0u8; (65_535 - 10) / 2];
    write(
        &s,
        vec![
            put(&longest, 2, b"long"),
            put(&zeros, 2, b"zeros"),
            put(b"after", 2, b""),
        ],
    );
    drop(s);
    let s = dir.open();
    assert_eq!(s.get_at(&longest, 2).unwrap().unwrap().value(), b"long");
    assert_eq!(s.get_at(&zeros, 2).unwrap().unwrap().value(), b"zeros");
    assert!(s.get_at(b"after", 2).unwrap().is_some());
}

#[test]
fn test_a_read_takes_the_newest_version_at_or_below_its_timestamp() {
    let dir = TestDir::new("read-at");
    let s = dir.open();
    write(&s, vec![put(b"k", 3, b"three"), put(b"k", 7, b"seven")]);
    assert_eq!(s.get_at(b"k", 5).unwrap().unwrap().version(), 3);
    assert_eq!(s.get_at(b"k", 7).unwrap().unwrap().value(), b"seven");
    assert_eq!(s.get_at(b"k", 2).unwrap(), None);
    assert_eq!(s.get_at(b"", 9).unwrap(), None);
    assert_eq!(s.get_at(b"k\x00", 9).unwrap(), None);
}

#[test]
fn test_a_deletion_hides_the_key() {
    let dir = TestDir::new("deletion");
    let s = dir.open();
    write(
        &s,
        vec![put(b"k", 3, b"v"), Entry::deletion(b"k".to_vec(), 6)],
    );
    assert_eq!(s.get_at(b"k", 7).unwrap(), None);
    assert_eq!(s.get_at(b"k", 4).unwrap().unwrap().version(), 3);
    assert_eq!(versions(&s, b"k", 7), Vec::<u64>::new());
    assert!(scanned(&s, &Scan::prefix(b"k".to_vec()), 7).is_empty());
    assert_eq!(versions(&s, b"k", 5), vec![3]);
}

#[test]
fn test_a_read_below_the_discard_timestamp_is_refused() {
    let dir = TestDir::new("too-old");
    let s = dir.open();
    write(&s, vec![put(b"k", 3, b"v")]);
    s.set_discard_ts(10).unwrap();
    assert_eq!(
        s.get_at(b"k", 5).unwrap_err().to_string(),
        "Transaction is too old"
    );
    assert_eq!(s.get_at(b"k", 9), Err(StoreError::TsTooOld()));
    assert_eq!(s.snapshot(9).err(), Some(StoreError::TsTooOld()));
    assert_eq!(s.snapshot(0).err(), Some(StoreError::TsTooOld()));
    assert_eq!(
        s.get_at(b"k", 10).unwrap().map(|i| i.version()),
        Some(3),
        "a read at the discard timestamp is served"
    );
}

#[test]
fn test_a_scan_is_unaffected_by_a_later_batch() {
    let dir = TestDir::new("scan-later");
    let s = dir.open();
    write(&s, vec![put(b"p1", 1, b"")]);
    let snapshot = s.snapshot(100).unwrap();
    let mut open = snapshot.scan(&Scan::prefix(b"p".to_vec()));
    write(
        &s,
        vec![put(b"p0", 2, b""), put(b"p2", 2, b""), put(b"p1", 3, b"")],
    );
    let first = open.next().unwrap().unwrap();
    assert!(
        open.next().is_none(),
        "the scan opened before the batch sees none of it"
    );
    assert_eq!((first.key(), first.version()), (&b"p1"[..], 1));
    let seen: Vec<Item> = snapshot
        .scan(&Scan::prefix(b"p".to_vec()))
        .map(Result::unwrap)
        .collect();
    assert_eq!(seen.len(), 1);
    assert_eq!(snapshot.get(b"p2").unwrap(), None);
}

#[test]
fn test_a_scan_reports_each_key_once_with_its_newest_version() {
    let dir = TestDir::new("scan-once");
    let s = dir.open();
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
    let of = |key: &[u8]| {
        all.iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| *v)
            .collect::<Vec<_>>()
    };
    assert_eq!(of(b"pa"), vec![4, 3, 2, 1]);
    assert_eq!(of(b"pc"), vec![6, 4, 2]);
    assert_eq!(
        scanned(&s, &Scan::range(b"pb".to_vec(), Some(b"pc".to_vec())), 20),
        vec![(b"pb".to_vec(), 14)]
    );
}

#[test]
fn test_a_reverse_scan_descends_with_versions_newest_first() {
    let dir = TestDir::new("reverse");
    let s = dir.open();
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
    let past = scanned(&s, &Scan::prefix(b"b".to_vec()).seek(b"z".to_vec()), 5);
    assert!(past.is_empty(), "a seek past the range finds nothing");
}

#[test]
fn test_a_batch_holds_entries_at_several_versions() {
    let dir = TestDir::new("several");
    let s = dir.open();
    write(&s, vec![put(b"a", 12, b""), put(b"b", 13, b"")]);
    let versions_at = |ts| -> Vec<Option<u64>> {
        s.snapshot(ts)
            .unwrap()
            .get_many(&[b"a", b"b"])
            .unwrap()
            .iter()
            .map(|i| i.as_ref().map(Item::version))
            .collect()
    };
    assert_eq!(versions_at(11), vec![None, None]);
    assert_eq!(versions_at(12), vec![Some(12), None]);
    assert_eq!(versions_at(13), vec![Some(12), Some(13)]);
}

#[test]
fn test_a_batch_records_the_highest_applied_timestamp() {
    let dir = TestDir::new("applied");
    let s = dir.open();
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
    drop(s);
    assert_eq!(dir.open().applied_ts(), 9);
}

#[test]
fn test_a_dropped_prefix_is_gone_after_reopening_and_below_the_drop() {
    let dir = TestDir::new("drop");
    let s = dir.open();
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
    drop(s);
    let s = dir.open();
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
    let dir = TestDir::new("drop-all");
    let s = dir.open();
    let mut batch = WriteBatch::new();
    batch.push(put(b"old", 3, b""));
    batch.set_applied_ts(3);
    s.apply(batch, PersistMode::Buffer).unwrap();
    s.set_discard_ts(2).unwrap();
    s.set_tombstone_horizon(1).unwrap();
    s.ban_namespace(5).unwrap();
    s.drop_all(vec![put(b"\x01schema", 1, b"s")]).unwrap();
    drop(s);
    let s = dir.open();
    let keys: Vec<Vec<u8>> = s.dump().unwrap().iter().map(|e| e.key().to_vec()).collect();
    assert_eq!(keys, vec![b"\x01schema".to_vec()]);
    assert_eq!(
        (s.applied_ts(), s.discard_ts(), s.tombstone_horizon()),
        (0, 0, 0)
    );
}

#[test]
fn test_a_banned_namespace_ignores_writes_and_hides_its_keys() {
    let dir = TestDir::new("banned");
    let s = dir.open();
    let (five, six) = (ns_key(5, b"\x00\x01a"), ns_key(6, b"\x00\x01a"));
    write(&s, vec![put(&five, 1, b"before")]);
    let before = s.snapshot(3).unwrap();
    s.ban_namespace(5).unwrap();
    write(&s, vec![put(&five, 2, b""), put(&six, 2, b"")]);
    assert_eq!(s.get_at(&five, 3).unwrap(), None);
    assert!(s.get_at(&six, 3).unwrap().is_some());
    assert!(
        scanned(&s, &Scan::prefix(vec![0x00]), 3)
            .iter()
            .all(|(k, _)| *k != five)
    );
    assert!(
        before.get(&five).unwrap().is_some(),
        "a snapshot opened before the ban still sees the key"
    );
    drop(before);
    assert_eq!(
        s.dump().unwrap().iter().filter(|e| e.key() == five).count(),
        1,
        "the ban hides the earlier version, it does not remove it"
    );
    drop(s);
    let s = dir.open();
    assert_eq!(
        s.get_at(&five, 3).unwrap(),
        None,
        "the ban survives a reopen"
    );
}

#[test]
fn test_dropping_everything_clears_the_banned_set() {
    let dir = TestDir::new("drop-all-ban");
    let s = dir.open();
    let five = ns_key(5, b"\x00\x01a");
    s.ban_namespace(5).unwrap();
    s.drop_all(Vec::new()).unwrap();
    write(&s, vec![put(&five, 1, b"")]);
    assert!(s.get_at(&five, 1).unwrap().is_some());
}

#[test]
fn test_a_second_open_of_a_held_directory_is_refused() {
    let dir = TestDir::new("locked");
    let _s = dir.open();
    let second = ultragraphdb_store_kv::FjallStore::open(
        dir.path(),
        ultragraphdb_store_kv::Durability::Process,
    );
    let err = second.unwrap_err();
    assert!(
        matches!(
            err,
            StoreError(ultragraphdb_error_store::StoreErrorEnum::OpenFailed(_))
        ),
        "{err}"
    );
}

#[test]
fn test_a_ban_covers_only_keys_longer_than_their_namespace() {
    // Badger's `isBanned` reads a namespace only from a key longer than the prefix byte and the eight namespace bytes
    // (`db.go:1921-1932`): a key of exactly nine bytes belongs to no namespace and is written under any ban.
    let dir = TestDir::new("banned-length");
    let s = dir.open();
    s.ban_namespace(5).unwrap();
    let nine = ns_key(5, b"");
    let ten = ns_key(5, b"x");
    let short = nine[..8].to_vec();
    write(
        &s,
        vec![
            put(&nine, 1, b"9"),
            put(&ten, 1, b"10"),
            put(&short, 1, b"8"),
        ],
    );
    assert_eq!(
        s.get_at(&nine, 1).unwrap().map(Item::into_value),
        Some(b"9".to_vec())
    );
    assert_eq!(
        s.get_at(&short, 1).unwrap().map(Item::into_value),
        Some(b"8".to_vec())
    );
    assert_eq!(s.get_at(&ten, 1).unwrap(), None);
    let keys: Vec<Vec<u8>> = s.dump().unwrap().iter().map(|e| e.key().to_vec()).collect();
    assert_eq!(
        keys,
        vec![short, nine],
        "the write to the banned namespace was dropped"
    );
}
