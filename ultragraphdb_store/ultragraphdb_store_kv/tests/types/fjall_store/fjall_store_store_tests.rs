/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store's contract (the kv-store specification) as the fjall store keeps it: the simulated store's contract
//! tests, run on fjall, with a crash as a close and a reopen.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::TERMS_KEYSPACE;
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

use ultragraphdb_tool_test::{TestDir, ns_key, put, scanned, versions, write};

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
    // In reverse the keys descend and each key's versions stay newest first.
    assert_eq!(
        scanned(&s, &scan.clone().reverse(), 9),
        vec![
            (b"\x02abc".to_vec(), 5),
            (b"\x02abc".to_vec(), 3),
            (b"\x02ab".to_vec(), 5),
            (b"\x02ab".to_vec(), 3),
            (b"\x02a\x00b".to_vec(), 5),
            (b"\x02a\x00b".to_vec(), 3),
        ]
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
    // Badger's exceedsSize dumps the first 1 << 10 bytes of the user key.
    assert_eq!(err, StoreError::KeyTooLarge(65_001, 65_000, vec![1; 1024]));
    assert!(
        err.to_string()
            .starts_with("Key with size 65001 exceeded 65000 limit. Key:\n00000000  01 01 01 01"),
        "{err}"
    );
    assert_eq!(s.get_at(b"fine", 1).unwrap(), None);

    let mut batch = WriteBatch::new();
    batch.push(put(b"fine", 1, b""));
    batch.push(put(&vec![0; 40_000], 1, b""));
    let err = s.apply(batch, PersistMode::Buffer).unwrap_err();
    // 40,000 zero bytes escaped to 80,000, the terminator's 2 and the version's 8; the head dumps the escaped bytes.
    let escaped_head: Vec<u8> = [0x00, 0xFF].repeat(512);
    assert_eq!(err, StoreError::KeyTooLarge(80_010, 65_535, escaped_head));
    assert!(
        err.to_string()
            .starts_with("Key with size 80010 exceeded 65535 limit. Key:\n00000000  00 ff 00 ff"),
        "{err}"
    );
    assert_eq!(s.get_at(b"fine", 1).unwrap(), None);
    assert!(
        !s.is_poisoned(),
        "a refused batch does not poison the store"
    );

    // The longest user key, and the longest stored key fjall's journal can hold, survive a reopen intact: 32,762 zero
    // bytes escaped to 65,524, one more byte, the terminator's 2 and the version's 8 make 65,535.
    let longest = vec![7u8; 65_000];
    let mut zeros = vec![0u8; 32_762];
    zeros.push(7);
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
    assert_eq!(s.get_at(b"after", 2).unwrap().map(|i| i.version()), Some(2));
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
    assert_eq!(
        seen,
        vec![Item::new(b"p1".to_vec(), 1, 0x04, false, Vec::new())]
    );
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
    assert_eq!(s.get_at(&six, 3).unwrap().map(|i| i.version()), Some(2));
    assert!(
        scanned(&s, &Scan::prefix(vec![0x00]), 3)
            .iter()
            .all(|(k, _)| *k != five)
    );
    assert_eq!(
        before.get(&five).unwrap().map(Item::into_value),
        Some(b"before".to_vec()),
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
    write(&s, vec![put(&five, 1, b"after")]);
    assert_eq!(
        s.get_at(&five, 1).unwrap().map(Item::into_value),
        Some(b"after".to_vec())
    );
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
    assert!(
        err.to_string().starts_with("Opening the store failed: "),
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

/// A batch of versionless writes, with one versioned entry beside them.
fn versionless_batch(writes: &[(&[u8], Option<&[u8]>)], version: u64) -> WriteBatch {
    let mut batch = WriteBatch::new();
    batch.push(put(&ns_key(0, b"data"), version, b"v"));
    for (key, value) in writes {
        match value {
            Some(value) => batch.put_versionless(key.to_vec(), value.to_vec()),
            None => batch.remove_versionless(key.to_vec()),
        }
    }
    batch
}

#[test]
fn test_versionless_writes_replace_remove_and_survive_a_reopen() {
    let dir = TestDir::new("versionless");
    let s = dir.open();
    s.apply(
        versionless_batch(
            &[(b"a", Some(b"1")), (b"b", Some(b"2")), (b"a", Some(b"3"))],
            1,
        ),
        PersistMode::Buffer,
    )
    .unwrap();
    s.apply(versionless_batch(&[(b"b", None)], 2), PersistMode::Sync)
        .unwrap();
    assert_eq!(
        s.get_versionless(b"a").unwrap(),
        Some(b"3".to_vec()),
        "the later write wins"
    );
    assert_eq!(s.get_versionless(b"b").unwrap(), None);
    drop(s);
    let s = dir.open();
    assert_eq!(
        s.dump_versionless()
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        vec![(b"a".to_vec(), b"3".to_vec())]
    );
}

#[test]
fn test_a_snapshot_reads_the_versionless_keys_of_its_opening() {
    let dir = TestDir::new("versionless-snapshot");
    let s = dir.open();
    s.apply(
        versionless_batch(&[(b"a", Some(b"1"))], 1),
        PersistMode::Buffer,
    )
    .unwrap();
    let snapshot = s.snapshot(1).unwrap();
    s.apply(
        versionless_batch(&[(b"a", Some(b"2")), (b"b", Some(b"3"))], 2),
        PersistMode::Buffer,
    )
    .unwrap();
    assert_eq!(snapshot.get_versionless(b"a").unwrap(), Some(b"1".to_vec()));
    assert_eq!(snapshot.get_versionless(b"b").unwrap(), None);
    assert_eq!(s.get_versionless(b"a").unwrap(), Some(b"2".to_vec()));
    // Below the discard timestamp a snapshot is refused; the store's own read is bounded by none.
    s.set_discard_ts(2).unwrap();
    drop(snapshot);
    assert_eq!(s.snapshot(1).err(), Some(StoreError::TsTooOld()));
    assert_eq!(s.get_versionless(b"b").unwrap(), Some(b"3".to_vec()));
}

#[test]
fn test_bans_prefix_drops_and_temporary_keyspaces_leave_versionless_keys_alone() {
    let dir = TestDir::new("versionless-kept");
    let s = dir.open();
    s.apply(
        versionless_batch(&[(&ns_key(5, b"t"), Some(b"1"))], 1),
        PersistMode::Buffer,
    )
    .unwrap();
    s.ban_namespace(5).unwrap();
    s.apply(
        versionless_batch(&[(&ns_key(5, b"u"), Some(b"2"))], 2),
        PersistMode::Buffer,
    )
    .unwrap();
    s.drop_prefixes(3, &[vec![0x00]], Vec::new()).unwrap();
    assert_eq!(
        s.get_versionless(&ns_key(5, b"t")).unwrap(),
        Some(b"1".to_vec())
    );
    assert_eq!(
        s.get_versionless(&ns_key(5, b"u")).unwrap(),
        Some(b"2".to_vec())
    );
    let temporary = s.create_temporary().unwrap();
    let mut batch = WriteBatch::new();
    batch.put_versionless(b"tmp".to_vec(), b"x".to_vec());
    s.apply_temporary(&temporary, batch).unwrap();
    let snapshot = s.temporary_snapshot(&temporary).unwrap();
    assert_eq!(snapshot.get_versionless(&ns_key(5, b"t")).unwrap(), None);
    assert_eq!(
        s.get_versionless(b"tmp").unwrap(),
        None,
        "a temporary batch's versionless writes are ignored"
    );
}

#[test]
fn test_versions_zero_and_the_maximum_order_newest_first_and_read_at_their_bounds() {
    // The complement of 0 is eight 0xFF bytes and of u64::MAX eight zero bytes: the ends of the version encoding.
    let dir = TestDir::new("version-bounds");
    let s = dir.open();
    let max = u64::MAX;
    write(
        &s,
        vec![
            put(b"k", 0, b"zero"),
            put(b"k", 1, b"one"),
            put(b"k", max - 1, b"below"),
            put(b"k", max, b"max"),
            put(b"k\x00", max, b"neighbour"),
        ],
    );
    let check = |s: &ultragraphdb_store_kv::FjallStore| {
        let value_at = |ts| s.get_at(b"k", ts).unwrap().map(Item::into_value);
        assert_eq!(value_at(max), Some(b"max".to_vec()));
        assert_eq!(value_at(max - 1), Some(b"below".to_vec()));
        assert_eq!(value_at(max - 2), Some(b"one".to_vec()));
        assert_eq!(value_at(1), Some(b"one".to_vec()));
        assert_eq!(value_at(0), Some(b"zero".to_vec()));
        assert_eq!(versions(s, b"k", max), vec![max, max - 1, 1, 0]);
        assert_eq!(versions(s, b"k", 0), vec![0]);
        assert_eq!(
            scanned(s, &Scan::prefix(b"k".to_vec()).reverse(), max),
            vec![(b"k\x00".to_vec(), max), (b"k".to_vec(), max)]
        );
    };
    check(&s);
    drop(s);
    check(&dir.open());
}

#[test]
fn test_a_read_at_a_versions_timestamp_and_one_either_side() {
    // Versions 10 and 20: a read at t sees the newest version at or below t, by the store's contract.
    let dir = TestDir::new("read-ts-bounds");
    let s = dir.open();
    write(&s, vec![put(b"k", 10, b"ten"), put(b"k", 20, b"twenty")]);
    for (ts, newest) in [
        (9, None),
        (10, Some(10)),
        (11, Some(10)),
        (19, Some(10)),
        (20, Some(20)),
        (21, Some(20)),
    ] {
        assert_eq!(
            s.get_at(b"k", ts).unwrap().map(|i| i.version()),
            newest,
            "get at {ts}"
        );
        let expected: Vec<(Vec<u8>, u64)> =
            newest.map(|v| (b"k".to_vec(), v)).into_iter().collect();
        assert_eq!(
            scanned(&s, &Scan::prefix(b"k".to_vec()), ts),
            expected,
            "scan at {ts}"
        );
        assert_eq!(
            scanned(&s, &Scan::prefix(b"k".to_vec()).reverse(), ts),
            expected,
            "reverse scan at {ts}"
        );
    }
    assert_eq!(versions(&s, b"k", 19), vec![10]);
    assert_eq!(versions(&s, b"k", 20), vec![20, 10]);
}

#[test]
fn test_empty_keys_and_keys_of_the_escape_bytes_keep_byte_order() {
    // Badger orders user keys bytewise (`y.CompareKeys`); the order below is the bytewise one, written by hand.
    let ordered: [&[u8]; 8] = [
        b"",
        b"\x00",
        b"\x00\x00",
        b"\x00\x01",
        b"\x01",
        b"\xff",
        b"\xff\x00",
        b"\xff\xff",
    ];
    let dir = TestDir::new("escape-order");
    let s = dir.open();
    // Written in reverse order, so that the store's order is not the order of the batch.
    for v in [1, 2] {
        write(
            &s,
            ordered
                .iter()
                .rev()
                .map(|k| put(k, v, &[*k, &[v as u8]].concat()))
                .collect(),
        );
    }
    let s = {
        drop(s);
        dir.open()
    };
    let all: Vec<(Vec<u8>, u64)> = ordered
        .iter()
        .flat_map(|k| [(k.to_vec(), 2), (k.to_vec(), 1)])
        .collect();
    assert_eq!(
        scanned(&s, &Scan::prefix(Vec::new()).all_versions(), 2),
        all
    );
    let reverse: Vec<(Vec<u8>, u64)> = ordered
        .iter()
        .rev()
        .flat_map(|k| [(k.to_vec(), 2), (k.to_vec(), 1)])
        .collect();
    assert_eq!(
        scanned(&s, &Scan::prefix(Vec::new()).reverse().all_versions(), 2),
        reverse
    );
    for k in ordered {
        for v in [1u64, 2] {
            assert_eq!(
                s.get_at(k, v).unwrap().map(Item::into_value),
                Some([k, &[v as u8][..]].concat::<u8>()),
                "{k:?} at {v}"
            );
        }
    }
    let keys = |scan: Scan| -> Vec<Vec<u8>> {
        scanned(&s, &scan, 2).into_iter().map(|(k, _)| k).collect()
    };
    assert_eq!(
        keys(Scan::prefix(vec![0x00])),
        vec![b"\x00".to_vec(), b"\x00\x00".to_vec(), b"\x00\x01".to_vec()],
        "the empty key does not begin with 0x00"
    );
    assert_eq!(
        keys(Scan::prefix(vec![0xFF])),
        vec![b"\xff".to_vec(), b"\xff\x00".to_vec(), b"\xff\xff".to_vec()]
    );
    assert_eq!(keys(Scan::key(b"\x00".to_vec())), vec![b"\x00".to_vec()]);
    assert_eq!(keys(Scan::key(Vec::new())), vec![Vec::<u8>::new()]);
    assert_eq!(
        keys(Scan::range(b"\x00".to_vec(), Some(b"\x00\x01".to_vec()))),
        vec![b"\x00".to_vec(), b"\x00\x00".to_vec()]
    );
    assert_eq!(
        keys(
            Scan::prefix(Vec::new())
                .reverse()
                .seek(b"\x00\x00".to_vec())
        ),
        vec![b"\x00\x00".to_vec(), b"\x00".to_vec(), Vec::<u8>::new()]
    );
    assert_eq!(
        keys(Scan::prefix(Vec::new()).seek(b"\x00\x00".to_vec())),
        vec![
            b"\x00\x00".to_vec(),
            b"\x00\x01".to_vec(),
            b"\x01".to_vec(),
            b"\xff".to_vec(),
            b"\xff\x00".to_vec(),
            b"\xff\xff".to_vec()
        ]
    );
}

#[test]
fn test_versionless_keys_of_the_escape_bytes_and_empty_values_are_stored_raw() {
    let dir = TestDir::new("versionless-raw");
    let s = dir.open();
    let writes: [(&[u8], &[u8]); 4] = [
        (b"\x00", b"zero"),
        (b"\x00\xff", b"escaped"),
        (b"\xff", b""),
        (b"a", b"\x00\x01"),
    ];
    let mut batch = WriteBatch::new();
    writes
        .iter()
        .for_each(|(k, v)| batch.put_versionless(k.to_vec(), v.to_vec()));
    batch.remove_versionless(b"absent".to_vec());
    s.apply(batch, PersistMode::Buffer).unwrap();
    assert_eq!(
        s.get_versionless(b"\xff").unwrap(),
        Some(Vec::new()),
        "an empty value is a value, not a removal"
    );
    assert_eq!(s.get_versionless(b"absent").unwrap(), None);
    assert_eq!(s.get_versionless(b"\x00\x00").unwrap(), None);
    let expected: std::collections::BTreeMap<Vec<u8>, Vec<u8>> = writes
        .iter()
        .map(|(k, v)| (k.to_vec(), v.to_vec()))
        .collect();
    assert_eq!(s.dump_versionless().unwrap(), expected);
    drop(s);
    // The keyspace holds each key as it is and its value raw, without the data keyspace's escaping and framing.
    let db = fjall::Database::builder(dir.path()).open().unwrap();
    let terms = db
        .keyspace(TERMS_KEYSPACE, fjall::KeyspaceCreateOptions::default)
        .unwrap();
    let raw: std::collections::BTreeMap<Vec<u8>, Vec<u8>> =
        fjall::Readable::iter(&db.snapshot(), &terms)
            .map(|g| g.into_inner().unwrap())
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect();
    assert_eq!(raw, expected);
}

#[test]
fn test_within_a_batch_the_later_versionless_write_of_a_key_wins() {
    let dir = TestDir::new("versionless-order");
    let s = dir.open();
    s.apply(
        versionless_batch(&[(b"a", Some(b"1"))], 1),
        PersistMode::Buffer,
    )
    .unwrap();
    s.apply(
        versionless_batch(
            &[
                (b"a", None),
                (b"a", Some(b"2")),
                (b"b", Some(b"1")),
                (b"b", None),
            ],
            2,
        ),
        PersistMode::Buffer,
    )
    .unwrap();
    assert_eq!(s.get_versionless(b"a").unwrap(), Some(b"2".to_vec()));
    assert_eq!(s.get_versionless(b"b").unwrap(), None);
    drop(s);
    let s = dir.open();
    assert_eq!(
        s.dump_versionless()
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        vec![(b"a".to_vec(), b"2".to_vec())]
    );
}

#[test]
fn test_an_oversized_key_refuses_the_versionless_writes_with_the_batch() {
    let dir = TestDir::new("versionless-oversized");
    let s = dir.open();
    // An oversized versioned key refuses the batch's versionless writes.
    let mut batch = WriteBatch::new();
    batch.put_versionless(b"t".to_vec(), b"1".to_vec());
    batch.push(put(&vec![1; 65_001], 1, b""));
    assert_eq!(
        s.apply(batch, PersistMode::Buffer),
        Err(StoreError::KeyTooLarge(65_001, 65_000, vec![1; 1024]))
    );
    assert_eq!(s.get_versionless(b"t").unwrap(), None);
    // An oversized versionless key refuses the batch's versioned entries, by the user key's limit and by the stored
    // key's, which counts a versionless key as a versioned one: 40,000 zero bytes are 80,010 once escaped and
    // versioned.
    for (key, error) in [
        (
            vec![2u8; 65_001],
            StoreError::KeyTooLarge(65_001, 65_000, vec![2; 1024]),
        ),
        (
            vec![0u8; 40_000],
            StoreError::KeyTooLarge(80_010, 65_535, [0x00, 0xFF].repeat(512)),
        ),
    ] {
        let mut batch = WriteBatch::new();
        batch.push(put(b"fine", 1, b""));
        batch.put_versionless(key, b"v".to_vec());
        batch.set_applied_ts(1);
        assert_eq!(s.apply(batch, PersistMode::Buffer), Err(error));
        assert_eq!(s.get_at(b"fine", 1).unwrap(), None);
        assert_eq!(s.applied_ts(), 0);
    }
    assert!(s.dump_versionless().unwrap().is_empty());
    assert!(!s.is_poisoned());
    // The longest versionless keys are written: 65,000 bytes, and 32,762 zero bytes and one more, 65,535 escaped and
    // versioned.
    let longest = vec![3u8; 65_000];
    let mut zeros = vec![0u8; 32_762];
    zeros.push(3);
    let mut batch = WriteBatch::new();
    batch.put_versionless(longest.clone(), b"long".to_vec());
    batch.put_versionless(zeros.clone(), b"zeros".to_vec());
    s.apply(batch, PersistMode::Buffer).unwrap();
    drop(s);
    let s = dir.open();
    assert_eq!(s.get_versionless(&longest).unwrap(), Some(b"long".to_vec()));
    assert_eq!(s.get_versionless(&zeros).unwrap(), Some(b"zeros".to_vec()));
}

#[test]
fn test_a_batch_of_versionless_writes_alone_records_its_applied_timestamp() {
    let dir = TestDir::new("versionless-applied");
    let s = dir.open();
    let mut batch = WriteBatch::new();
    batch.put_versionless(b"t".to_vec(), b"1".to_vec());
    batch.set_applied_ts(5);
    s.apply(batch, PersistMode::Buffer).unwrap();
    assert_eq!(s.applied_ts(), 5);
    drop(s);
    let s = dir.open();
    assert_eq!(
        (s.applied_ts(), s.get_versionless(b"t").unwrap()),
        (5, Some(b"1".to_vec()))
    );
    assert!(s.dump().unwrap().is_empty());
}

#[test]
fn test_a_snapshot_reads_versionless_keys_whatever_its_read_timestamp() {
    // A versionless key carries no version: a snapshot reading below the batch's versioned entries still sees it.
    let dir = TestDir::new("versionless-read-ts");
    let s = dir.open();
    let mut batch = WriteBatch::new();
    batch.push(put(b"k", 9, b"nine"));
    batch.put_versionless(b"t".to_vec(), b"1".to_vec());
    s.apply(batch, PersistMode::Buffer).unwrap();
    let snapshot = s.snapshot(1).unwrap();
    assert_eq!(snapshot.get(b"k").unwrap(), None);
    assert_eq!(snapshot.get_versionless(b"t").unwrap(), Some(b"1".to_vec()));
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the readers run on threads beside the writer, as the production store's readers do"
)]
fn test_readers_see_a_batchs_versionless_and_versioned_writes_together() {
    // Readers take snapshot after snapshot while one batch writes 10,000 keys of each kind: every snapshot sees the
    // first and the last of both kinds, or none of them.
    use std::sync::atomic::{AtomicBool, Ordering};
    let dir = TestDir::new("versionless-whole");
    let s = std::sync::Arc::new(dir.open());
    let done = AtomicBool::new(false);
    let observed: Vec<[bool; 4]> = std::thread::scope(|scope| {
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let (s, done) = (s.clone(), &done);
                scope.spawn(move || {
                    let mut seen = Vec::new();
                    loop {
                        let finished = done.load(Ordering::Acquire);
                        let snapshot = s.snapshot(10).unwrap();
                        seen.push([
                            snapshot.get(b"k0").unwrap().is_some(),
                            snapshot.get(b"k9999").unwrap().is_some(),
                            snapshot.get_versionless(b"t0").unwrap().is_some(),
                            snapshot.get_versionless(b"t9999").unwrap().is_some(),
                        ]);
                        if finished {
                            return seen;
                        }
                    }
                })
            })
            .collect();
        let mut batch = WriteBatch::new();
        for i in 0..10_000 {
            batch.push(put(format!("k{i}").as_bytes(), 3, b""));
            batch.put_versionless(format!("t{i}").into_bytes(), vec![1]);
        }
        s.apply(batch, PersistMode::Buffer).unwrap();
        done.store(true, Ordering::Release);
        readers
            .into_iter()
            .flat_map(|r| r.join().unwrap())
            .collect()
    });
    let torn: Vec<&[bool; 4]> = observed
        .iter()
        .filter(|o| *o != &[true; 4] && *o != &[false; 4])
        .collect();
    assert!(torn.is_empty(), "snapshots saw part of the batch: {torn:?}");
    assert_eq!(
        observed.last(),
        Some(&[true; 4]),
        "the last reads follow the batch"
    );
    println!(
        "{} snapshots, {} before the batch",
        observed.len(),
        observed.iter().filter(|o| *o == &[false; 4]).count()
    );
}

#[test]
fn test_an_empty_versionless_key_refuses_its_batch_put_or_removed() {
    // fjall's batches refuse an empty key with a panic; the store refuses it first, with Badger's ErrEmptyKey, and the
    // batch writes nothing.
    let dir = TestDir::new("versionless-empty-key");
    let s = dir.open();
    let empty_key_batch = |remove: bool| {
        let mut batch = WriteBatch::new();
        batch.push(put(b"k", 1, b"v"));
        batch.put_versionless(b"t".to_vec(), b"1".to_vec());
        if remove {
            batch.remove_versionless(Vec::new());
        } else {
            batch.put_versionless(Vec::new(), b"v".to_vec());
        }
        batch.set_applied_ts(1);
        batch
    };
    for remove in [false, true] {
        let refused = s.apply(empty_key_batch(remove), PersistMode::Buffer);
        assert_eq!(refused, Err(StoreError::EmptyKey()), "removal: {remove}");
        assert_eq!(refused.unwrap_err().to_string(), "Key cannot be empty");
    }
    let temporary = s.create_temporary().unwrap();
    assert_eq!(
        s.apply_temporary(&temporary, empty_key_batch(false)),
        Err(StoreError::EmptyKey())
    );
    assert!(!s.is_poisoned());
    assert_eq!(
        s.temporary_snapshot(&temporary).unwrap().get(b"k").unwrap(),
        None
    );
    drop(s);
    let s = dir.open();
    assert_eq!(
        (
            s.dump().unwrap(),
            s.dump_versionless().unwrap().len(),
            s.applied_ts()
        ),
        (Vec::new(), 0, 0)
    );
}
