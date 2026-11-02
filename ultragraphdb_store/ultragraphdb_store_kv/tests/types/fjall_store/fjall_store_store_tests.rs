/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store's contract (the kv-store specification) as the fjall store keeps it: the simulated store's contract
//! tests, run on fjall, with a crash as a close and a reopen.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::{FjallStore, TERMS_KEYSPACE};
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_test::{TestDir, ns_key, put, scanned, versions, write};

/// `len` bytes of `0x00 0x01` repeated: every zero byte a run of one, the densest the stored key's codes get.
fn alternating(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 2) as u8).collect()
}

/// The first 1,024 stored bytes of such a key: `0x00 0xFE 0x01` repeated.
fn dense_head() -> Vec<u8> {
    [0x00, 0xFE, 0x01]
        .iter()
        .copied()
        .cycle()
        .take(1024)
        .collect()
}

#[test]
fn test_a_keys_versions_iterate_newest_first_with_their_meta() -> TestResult {
    let dir = TestDir::new("versions");
    let s = dir.open()?;
    for v in [3, 7, 5] {
        write(&s, vec![Entry::put(b"k".to_vec(), v, v as u8, vec![])])?;
    }
    let snapshot = s.snapshot(10)?;
    let items: Vec<(u64, u8)> = snapshot
        .versions(b"k")
        .map(|i| i.map(|i| (i.version(), i.meta())))
        .collect::<Result<_, _>>()?;
    assert_eq!(items, vec![(7, 7), (5, 5), (3, 3)]);
    Ok(())
}

#[test]
fn test_a_version_below_the_newest_can_be_written() -> TestResult {
    let dir = TestDir::new("below");
    let s = dir.open()?;
    write(&s, vec![put(b"k", 9, b"a")])?;
    write(&s, vec![put(b"k", 6, b"b")])?;
    assert_eq!(versions(&s, b"k", 10)?, vec![9, 6]);
    Ok(())
}

#[test]
fn test_rewriting_a_version_replaces_it_across_batches_and_within_one() -> TestResult {
    let dir = TestDir::new("rewrite");
    let s = dir.open()?;
    write(&s, vec![put(b"k", 4, b"first")])?;
    write(&s, vec![put(b"k", 4, b"second")])?;
    assert_eq!(
        s.get_at(b"k", 4)?.context("the key\'s item")?.value(),
        b"second"
    );
    assert_eq!(versions(&s, b"k", 4)?, vec![4]);
    write(&s, vec![put(b"j", 4, b"first"), put(b"j", 4, b"later")])?;
    assert_eq!(
        s.get_at(b"j", 4)?.context("the key\'s item")?.value(),
        b"later"
    );
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        reopened
            .get_at(b"j", 4)?
            .context("the key\'s item")?
            .value(),
        b"later",
        "the journal replays the later one"
    );
    assert_eq!(
        reopened
            .get_at(b"k", 4)?
            .context("the key\'s item")?
            .value(),
        b"second"
    );
    Ok(())
}

#[test]
fn test_a_key_that_prefixes_another_sorts_first() -> TestResult {
    let dir = TestDir::new("prefix-order");
    let s = dir.open()?;
    for v in [3, 5] {
        write(
            &s,
            vec![
                put(b"\x02ab", v, b""),
                put(b"\x02abc", v, b""),
                put(b"\x02a\x00b", v, b""),
            ],
        )?;
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
    assert_eq!(scanned(&s, &scan, 9)?, expected);
    // In reverse the keys descend and each key's versions stay newest first.
    assert_eq!(
        scanned(&s, &scan.clone().reverse(), 9)?,
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
        scanned(&s, &Scan::prefix(b"\x02ab".to_vec()), 9)?,
        vec![(b"\x02ab".to_vec(), 5), (b"\x02abc".to_vec(), 5)],
        "ab and abc, not a\\0b"
    );
    Ok(())
}

#[test]
fn test_an_oversized_key_refuses_its_whole_batch() -> TestResult {
    let dir = TestDir::new("oversized");
    let s = dir.open()?;
    let mut batch = WriteBatch::new();
    batch.push(put(b"fine", 1, b""));
    batch.push(put(&vec![1; 65_001], 1, b""));
    let err = s
        .apply(batch, PersistMode::Buffer)
        .err()
        .context("the expected error")?;
    // Badger's exceedsSize dumps the first 1 << 10 bytes of the user key.
    assert_eq!(
        err,
        StoreError::key_too_large(65_001, 65_000, vec![1; 1024])
    );
    assert!(
        err.to_string()
            .starts_with("Key with size 65001 exceeded 65000 limit. Key:\n00000000  01 01 01 01"),
        "{err}"
    );
    assert_eq!(s.get_at(b"fine", 1)?, None);

    let mut dense_batch = WriteBatch::new();
    dense_batch.push(put(b"fine", 1, b""));
    dense_batch.push(put(&alternating(44_000), 1, b""));
    let dense_err = s
        .apply(dense_batch, PersistMode::Buffer)
        .err()
        .context("the expected error")?;
    // 22,000 zero bytes, each a run of one before a one, encode to 66,000 bytes, the end code's 2 and the version's 8;
    // the head dumps the encoded bytes.
    assert_eq!(
        dense_err,
        StoreError::key_too_large(66_010, 65_535, dense_head())
    );
    assert!(
        dense_err
            .to_string()
            .starts_with("Key with size 66010 exceeded 65535 limit. Key:\n00000000  00 fe 01 00"),
        "{dense_err}"
    );
    assert_eq!(s.get_at(b"fine", 1)?, None);
    assert!(
        !s.is_poisoned(),
        "a refused batch does not poison the store"
    );

    // The longest user key, and the longest stored key fjall's journal can hold, survive a reopen intact: 21,841 zero
    // bytes each before a one encode to 65,523, two more ones, the end code's 2 and the version's 8 make 65,535.
    let longest = vec![7u8; 65_000];
    let zeros = [alternating(43_682), vec![1, 1]].concat();
    write(
        &s,
        vec![
            put(&longest, 2, b"long"),
            put(&zeros, 2, b"zeros"),
            put(b"after", 2, b""),
        ],
    )?;
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        reopened
            .get_at(&longest, 2)?
            .context("the key\'s item")?
            .value(),
        b"long"
    );
    assert_eq!(
        reopened
            .get_at(&zeros, 2)?
            .context("the key\'s item")?
            .value(),
        b"zeros"
    );
    assert_eq!(reopened.get_at(b"after", 2)?.map(|i| i.version()), Some(2));
    Ok(())
}

#[test]
fn test_a_read_takes_the_newest_version_at_or_below_its_timestamp() -> TestResult {
    let dir = TestDir::new("read-at");
    let s = dir.open()?;
    write(&s, vec![put(b"k", 3, b"three"), put(b"k", 7, b"seven")])?;
    assert_eq!(s.get_at(b"k", 5)?.context("the key\'s item")?.version(), 3);
    assert_eq!(
        s.get_at(b"k", 7)?.context("the key\'s item")?.value(),
        b"seven"
    );
    assert_eq!(s.get_at(b"k", 2)?, None);
    assert_eq!(s.get_at(b"", 9)?, None);
    assert_eq!(s.get_at(b"k\x00", 9)?, None);
    Ok(())
}

#[test]
fn test_a_deletion_hides_the_key() -> TestResult {
    let dir = TestDir::new("deletion");
    let s = dir.open()?;
    write(
        &s,
        vec![put(b"k", 3, b"v"), Entry::deletion(b"k".to_vec(), 6)],
    )?;
    assert_eq!(s.get_at(b"k", 7)?, None);
    assert_eq!(s.get_at(b"k", 4)?.context("the key\'s item")?.version(), 3);
    assert_eq!(versions(&s, b"k", 7)?, Vec::<u64>::new());
    assert!(scanned(&s, &Scan::prefix(b"k".to_vec()), 7)?.is_empty());
    assert_eq!(versions(&s, b"k", 5)?, vec![3]);
    Ok(())
}

#[test]
fn test_a_read_below_the_discard_timestamp_is_refused() -> TestResult {
    let dir = TestDir::new("too-old");
    let s = dir.open()?;
    write(&s, vec![put(b"k", 3, b"v")])?;
    s.set_discard_ts(10)?;
    assert_eq!(
        s.get_at(b"k", 5)
            .err()
            .context("the expected error")?
            .to_string(),
        "Transaction is too old"
    );
    assert_eq!(s.get_at(b"k", 9), Err(StoreError::ts_too_old()));
    assert_eq!(s.snapshot(9).err(), Some(StoreError::ts_too_old()));
    assert_eq!(s.snapshot(0).err(), Some(StoreError::ts_too_old()));
    assert_eq!(
        s.get_at(b"k", 10)?.map(|i| i.version()),
        Some(3),
        "a read at the discard timestamp is served"
    );
    Ok(())
}

#[test]
fn test_a_scan_is_unaffected_by_a_later_batch() -> TestResult {
    let dir = TestDir::new("scan-later");
    let s = dir.open()?;
    write(&s, vec![put(b"p1", 1, b"")])?;
    let snapshot = s.snapshot(100)?;
    let mut open = snapshot.scan(&Scan::prefix(b"p".to_vec()));
    write(
        &s,
        vec![put(b"p0", 2, b""), put(b"p2", 2, b""), put(b"p1", 3, b"")],
    )?;
    let first = open.next().context("a next item")??;
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
    assert_eq!(snapshot.get(b"p2")?, None);
    Ok(())
}

#[test]
fn test_a_scan_reports_each_key_once_with_its_newest_version() -> TestResult {
    let dir = TestDir::new("scan-once");
    let s = dir.open()?;
    for v in 1..=4 {
        write(
            &s,
            vec![
                put(b"pa", v, b""),
                put(b"pb", v + 10, b""),
                put(b"pc", v * 2, b""),
            ],
        )?;
    }
    let got = scanned(&s, &Scan::prefix(b"p".to_vec()), 6)?;
    assert_eq!(got, vec![(b"pa".to_vec(), 4), (b"pc".to_vec(), 6)]);
    let all = scanned(&s, &Scan::prefix(b"p".to_vec()).all_versions(), 6)?;
    let of = |key: &[u8]| {
        all.iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| *v)
            .collect::<Vec<_>>()
    };
    assert_eq!(of(b"pa"), vec![4, 3, 2, 1]);
    assert_eq!(of(b"pc"), vec![6, 4, 2]);
    assert_eq!(
        scanned(&s, &Scan::range(b"pb".to_vec(), Some(b"pc".to_vec())), 20)?,
        vec![(b"pb".to_vec(), 14)]
    );
    Ok(())
}

#[test]
fn test_a_reverse_scan_descends_with_versions_newest_first() -> TestResult {
    let dir = TestDir::new("reverse");
    let s = dir.open()?;
    write(
        &s,
        vec![put(b"a", 1, b""), put(b"b", 1, b""), put(b"c", 1, b"")],
    )?;
    write(&s, vec![put(b"b", 2, b"")])?;
    let all = scanned(&s, &Scan::prefix(Vec::new()).reverse().all_versions(), 5)?;
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
    )?;
    assert_eq!(sought, vec![(b"b".to_vec(), 2), (b"a".to_vec(), 1)]);
    let forward = scanned(&s, &Scan::prefix(Vec::new()).seek(b"b".to_vec()), 5)?;
    assert_eq!(forward, vec![(b"b".to_vec(), 2), (b"c".to_vec(), 1)]);
    let past = scanned(&s, &Scan::prefix(b"b".to_vec()).seek(b"z".to_vec()), 5)?;
    assert!(past.is_empty(), "a seek past the range finds nothing");
    Ok(())
}

#[test]
fn test_a_batch_holds_entries_at_several_versions() -> TestResult {
    let dir = TestDir::new("several");
    let s = dir.open()?;
    write(&s, vec![put(b"a", 12, b""), put(b"b", 13, b"")])?;
    let versions_at = |ts| -> TestResult<Vec<Option<u64>>> {
        Ok(s.snapshot(ts)?
            .get_many(&[b"a", b"b"])?
            .iter()
            .map(|i| i.as_ref().map(Item::version))
            .collect())
    };
    assert_eq!(versions_at(11)?, vec![None, None]);
    assert_eq!(versions_at(12)?, vec![Some(12), None]);
    assert_eq!(versions_at(13)?, vec![Some(12), Some(13)]);
    Ok(())
}

#[test]
fn test_a_batch_records_the_highest_applied_timestamp() -> TestResult {
    let dir = TestDir::new("applied");
    let s = dir.open()?;
    let mut batch = WriteBatch::new();
    batch.set_applied_ts(9);
    s.apply(batch, PersistMode::Buffer)?;
    let mut rollup = WriteBatch::new();
    rollup.push(put(b"k", 4, b""));
    s.apply(rollup, PersistMode::Buffer)?;
    let mut older = WriteBatch::new();
    older.set_applied_ts(5);
    s.apply(older, PersistMode::Buffer)?;
    assert_eq!(s.applied_ts(), 9);
    drop(s);
    assert_eq!(dir.open()?.applied_ts(), 9);
    Ok(())
}

#[test]
fn test_a_dropped_prefix_is_gone_after_reopening_and_below_the_drop() -> TestResult {
    let dir = TestDir::new("drop");
    let s = dir.open()?;
    write(
        &s,
        vec![
            put(b"\x00a1", 3, b""),
            put(b"\x00a2", 3, b""),
            put(b"\x00b1", 3, b""),
        ],
    )?;
    s.drop_prefixes(8, &[b"\x00a".to_vec()], vec![put(b"\x00a9", 1, b"initial")])?;
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        reopened.get_at(b"\x00a1", 5)?,
        None,
        "a read below the drop finds nothing of it"
    );
    assert_eq!(reopened.get_at(b"\x00a2", 9)?, None);
    assert_eq!(
        reopened
            .get_at(b"\x00b1", 9)?
            .context("the key\'s item")?
            .version(),
        3
    );
    assert_eq!(
        reopened
            .get_at(b"\x00a9", 9)?
            .context("the key\'s item")?
            .value(),
        b"initial"
    );
    Ok(())
}

#[test]
fn test_dropping_everything_keeps_only_its_entries_and_clears_the_metadata() -> TestResult {
    let dir = TestDir::new("drop-all");
    let s = dir.open()?;
    let mut batch = WriteBatch::new();
    batch.push(put(b"old", 3, b""));
    batch.set_applied_ts(3);
    s.apply(batch, PersistMode::Buffer)?;
    s.set_discard_ts(2)?;
    s.set_tombstone_horizon(1)?;
    s.ban_namespace(5)?;
    s.drop_all(vec![put(b"\x01schema", 1, b"s")])?;
    drop(s);
    let reopened = dir.open()?;
    let keys: Vec<Vec<u8>> = reopened.dump()?.iter().map(|e| e.key().to_vec()).collect();
    assert_eq!(keys, vec![b"\x01schema".to_vec()]);
    assert_eq!(
        (
            reopened.applied_ts(),
            reopened.discard_ts(),
            reopened.tombstone_horizon()
        ),
        (0, 0, 0)
    );
    Ok(())
}

#[test]
fn test_a_banned_namespace_ignores_writes_and_hides_its_keys() -> TestResult {
    let dir = TestDir::new("banned");
    let s = dir.open()?;
    let (five, six) = (ns_key(5, b"\x00\x01a"), ns_key(6, b"\x00\x01a"));
    write(&s, vec![put(&five, 1, b"before")])?;
    let before = s.snapshot(3)?;
    s.ban_namespace(5)?;
    write(&s, vec![put(&five, 2, b""), put(&six, 2, b"")])?;
    assert_eq!(s.get_at(&five, 3)?, None);
    assert_eq!(s.get_at(&six, 3)?.map(|i| i.version()), Some(2));
    assert!(
        scanned(&s, &Scan::prefix(vec![0x00]), 3)?
            .iter()
            .all(|(k, _)| *k != five)
    );
    assert_eq!(
        before.get(&five)?.map(Item::into_value),
        Some(b"before".to_vec()),
        "a snapshot opened before the ban still sees the key"
    );
    drop(before);
    assert_eq!(
        s.dump()?.iter().filter(|e| e.key() == five).count(),
        1,
        "the ban hides the earlier version, it does not remove it"
    );
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        reopened.get_at(&five, 3)?,
        None,
        "the ban survives a reopen"
    );
    Ok(())
}

#[test]
fn test_dropping_everything_clears_the_banned_set() -> TestResult {
    let dir = TestDir::new("drop-all-ban");
    let s = dir.open()?;
    let five = ns_key(5, b"\x00\x01a");
    s.ban_namespace(5)?;
    s.drop_all(Vec::new())?;
    write(&s, vec![put(&five, 1, b"after")])?;
    assert_eq!(
        s.get_at(&five, 1)?.map(Item::into_value),
        Some(b"after".to_vec())
    );
    Ok(())
}

#[test]
fn test_a_second_open_of_a_held_directory_is_refused() -> TestResult {
    let dir = TestDir::new("locked");
    let _s = dir.open()?;
    let second = ultragraphdb_store_kv::FjallStore::open(
        dir.path(),
        ultragraphdb_store_kv::Durability::Process,
    );
    let err = second.err().context("the expected error")?;
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
    Ok(())
}

#[test]
fn test_a_ban_covers_only_keys_longer_than_their_namespace() -> TestResult {
    // Badger's `isBanned` reads a namespace only from a key longer than the prefix byte and the eight namespace bytes
    // (`db.go:1921-1932`): a key of exactly nine bytes belongs to no namespace and is written under any ban.
    let dir = TestDir::new("banned-length");
    let s = dir.open()?;
    s.ban_namespace(5)?;
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
    )?;
    assert_eq!(
        s.get_at(&nine, 1)?.map(Item::into_value),
        Some(b"9".to_vec())
    );
    assert_eq!(
        s.get_at(&short, 1)?.map(Item::into_value),
        Some(b"8".to_vec())
    );
    assert_eq!(s.get_at(&ten, 1)?, None);
    let keys: Vec<Vec<u8>> = s.dump()?.iter().map(|e| e.key().to_vec()).collect();
    assert_eq!(
        keys,
        vec![short, nine],
        "the write to the banned namespace was dropped"
    );
    Ok(())
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
fn test_versionless_writes_replace_remove_and_survive_a_reopen() -> TestResult {
    let dir = TestDir::new("versionless");
    let s = dir.open()?;
    s.apply(
        versionless_batch(
            &[(b"a", Some(b"1")), (b"b", Some(b"2")), (b"a", Some(b"3"))],
            1,
        ),
        PersistMode::Buffer,
    )?;
    s.apply(versionless_batch(&[(b"b", None)], 2), PersistMode::Sync)?;
    assert_eq!(
        s.get_versionless(b"a")?,
        Some(b"3".to_vec()),
        "the later write wins"
    );
    assert_eq!(s.get_versionless(b"b")?, None);
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        reopened.dump_versionless()?.into_iter().collect::<Vec<_>>(),
        vec![(b"a".to_vec(), b"3".to_vec())]
    );
    Ok(())
}

#[test]
fn test_a_snapshot_reads_the_versionless_keys_of_its_opening() -> TestResult {
    let dir = TestDir::new("versionless-snapshot");
    let s = dir.open()?;
    s.apply(
        versionless_batch(&[(b"a", Some(b"1"))], 1),
        PersistMode::Buffer,
    )?;
    let snapshot = s.snapshot(1)?;
    s.apply(
        versionless_batch(&[(b"a", Some(b"2")), (b"b", Some(b"3"))], 2),
        PersistMode::Buffer,
    )?;
    assert_eq!(snapshot.get_versionless(b"a")?, Some(b"1".to_vec()));
    assert_eq!(snapshot.get_versionless(b"b")?, None);
    assert_eq!(s.get_versionless(b"a")?, Some(b"2".to_vec()));
    // Below the discard timestamp a snapshot is refused; the store's own read is bounded by none.
    s.set_discard_ts(2)?;
    drop(snapshot);
    assert_eq!(s.snapshot(1).err(), Some(StoreError::ts_too_old()));
    assert_eq!(s.get_versionless(b"b")?, Some(b"3".to_vec()));
    Ok(())
}

#[test]
fn test_bans_prefix_drops_and_temporary_keyspaces_leave_versionless_keys_alone() -> TestResult {
    let dir = TestDir::new("versionless-kept");
    let s = dir.open()?;
    s.apply(
        versionless_batch(&[(&ns_key(5, b"t"), Some(b"1"))], 1),
        PersistMode::Buffer,
    )?;
    s.ban_namespace(5)?;
    s.apply(
        versionless_batch(&[(&ns_key(5, b"u"), Some(b"2"))], 2),
        PersistMode::Buffer,
    )?;
    s.drop_prefixes(3, &[vec![0x00]], Vec::new())?;
    assert_eq!(s.get_versionless(&ns_key(5, b"t"))?, Some(b"1".to_vec()));
    assert_eq!(s.get_versionless(&ns_key(5, b"u"))?, Some(b"2".to_vec()));
    let temporary = s.create_temporary()?;
    let mut batch = WriteBatch::new();
    batch.put_versionless(b"tmp".to_vec(), b"x".to_vec());
    s.apply_temporary(&temporary, batch)?;
    let snapshot = s.temporary_snapshot(&temporary)?;
    assert_eq!(snapshot.get_versionless(&ns_key(5, b"t"))?, None);
    assert_eq!(
        s.get_versionless(b"tmp")?,
        None,
        "a temporary batch's versionless writes are ignored"
    );
    Ok(())
}

#[test]
fn test_versions_zero_and_the_maximum_order_newest_first_and_read_at_their_bounds() -> TestResult {
    // The complement of 0 is eight 0xFF bytes and of u64::MAX eight zero bytes: the ends of the version encoding.
    let dir = TestDir::new("version-bounds");
    let s = dir.open()?;
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
    )?;
    let check = |store: &ultragraphdb_store_kv::FjallStore| -> TestResult {
        let value_at = |ts| store.get_at(b"k", ts).map(|i| i.map(Item::into_value));
        assert_eq!(value_at(max)?, Some(b"max".to_vec()));
        assert_eq!(value_at(max - 1)?, Some(b"below".to_vec()));
        assert_eq!(value_at(max - 2)?, Some(b"one".to_vec()));
        assert_eq!(value_at(1)?, Some(b"one".to_vec()));
        assert_eq!(value_at(0)?, Some(b"zero".to_vec()));
        assert_eq!(versions(store, b"k", max)?, vec![max, max - 1, 1, 0]);
        assert_eq!(versions(store, b"k", 0)?, vec![0]);
        assert_eq!(
            scanned(store, &Scan::prefix(b"k".to_vec()).reverse(), max)?,
            vec![(b"k\x00".to_vec(), max), (b"k".to_vec(), max)]
        );
        Ok(())
    };
    check(&s)?;
    drop(s);
    check(&dir.open()?)?;
    Ok(())
}

#[test]
fn test_a_read_at_a_versions_timestamp_and_one_either_side() -> TestResult {
    // Versions 10 and 20: a read at t sees the newest version at or below t, by the store's contract.
    let dir = TestDir::new("read-ts-bounds");
    let s = dir.open()?;
    write(&s, vec![put(b"k", 10, b"ten"), put(b"k", 20, b"twenty")])?;
    for (ts, newest) in [
        (9, None),
        (10, Some(10)),
        (11, Some(10)),
        (19, Some(10)),
        (20, Some(20)),
        (21, Some(20)),
    ] {
        assert_eq!(
            s.get_at(b"k", ts)?.map(|i| i.version()),
            newest,
            "get at {ts}"
        );
        let expected: Vec<(Vec<u8>, u64)> =
            newest.map(|v| (b"k".to_vec(), v)).into_iter().collect();
        assert_eq!(
            scanned(&s, &Scan::prefix(b"k".to_vec()), ts)?,
            expected,
            "scan at {ts}"
        );
        assert_eq!(
            scanned(&s, &Scan::prefix(b"k".to_vec()).reverse(), ts)?,
            expected,
            "reverse scan at {ts}"
        );
    }
    assert_eq!(versions(&s, b"k", 19)?, vec![10]);
    assert_eq!(versions(&s, b"k", 20)?, vec![20, 10]);
    Ok(())
}

#[test]
fn test_empty_keys_and_keys_of_zero_and_code_bytes_keep_byte_order() -> TestResult {
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
    let dir = TestDir::new("code-order");
    let s = dir.open()?;
    // Written in reverse order, so that the store's order is not the order of the batch.
    for v in [1, 2] {
        write(
            &s,
            ordered
                .iter()
                .rev()
                .map(|k| put(k, v, &[*k, &[v as u8]].concat()))
                .collect(),
        )?;
    }
    let s = {
        drop(s);
        dir.open()?
    };
    let all: Vec<(Vec<u8>, u64)> = ordered
        .iter()
        .flat_map(|k| [(k.to_vec(), 2), (k.to_vec(), 1)])
        .collect();
    assert_eq!(
        scanned(&s, &Scan::prefix(Vec::new()).all_versions(), 2)?,
        all
    );
    let reverse: Vec<(Vec<u8>, u64)> = ordered
        .iter()
        .rev()
        .flat_map(|k| [(k.to_vec(), 2), (k.to_vec(), 1)])
        .collect();
    assert_eq!(
        scanned(&s, &Scan::prefix(Vec::new()).reverse().all_versions(), 2)?,
        reverse
    );
    for k in ordered {
        for v in [1u64, 2] {
            assert_eq!(
                s.get_at(k, v)?.map(Item::into_value),
                Some([k, &[v as u8][..]].concat::<u8>()),
                "{k:?} at {v}"
            );
        }
    }
    assert_scans_keep_byte_order(&s)?;
    Ok(())
}

/// Checks the scans of prefixes, keys, ranges and seeks over the keys of
/// `test_empty_keys_and_keys_of_zero_and_code_bytes_keep_byte_order` held by `s`.
fn assert_scans_keep_byte_order(s: &FjallStore) -> TestResult {
    let keys = |scan: Scan| -> TestResult<Vec<Vec<u8>>> {
        Ok(scanned(s, &scan, 2)?.into_iter().map(|(k, _)| k).collect())
    };
    assert_eq!(
        keys(Scan::prefix(vec![0x00]))?,
        vec![b"\x00".to_vec(), b"\x00\x00".to_vec(), b"\x00\x01".to_vec()],
        "the empty key does not begin with 0x00"
    );
    assert_eq!(
        keys(Scan::prefix(vec![0xFF]))?,
        vec![b"\xff".to_vec(), b"\xff\x00".to_vec(), b"\xff\xff".to_vec()]
    );
    assert_eq!(keys(Scan::key(b"\x00".to_vec()))?, vec![b"\x00".to_vec()]);
    assert_eq!(keys(Scan::key(Vec::new()))?, vec![Vec::<u8>::new()]);
    assert_eq!(
        keys(Scan::range(b"\x00".to_vec(), Some(b"\x00\x01".to_vec())))?,
        vec![b"\x00".to_vec(), b"\x00\x00".to_vec()]
    );
    assert_eq!(
        keys(
            Scan::prefix(Vec::new())
                .reverse()
                .seek(b"\x00\x00".to_vec())
        )?,
        vec![b"\x00\x00".to_vec(), b"\x00".to_vec(), Vec::<u8>::new()]
    );
    assert_eq!(
        keys(Scan::prefix(Vec::new()).seek(b"\x00\x00".to_vec()))?,
        vec![
            b"\x00\x00".to_vec(),
            b"\x00\x01".to_vec(),
            b"\x01".to_vec(),
            b"\xff".to_vec(),
            b"\xff\x00".to_vec(),
            b"\xff\xff".to_vec()
        ]
    );
    Ok(())
}

#[test]
fn test_versionless_keys_of_zero_and_code_bytes_and_empty_values_are_stored_raw() -> TestResult {
    let dir = TestDir::new("versionless-raw");
    let s = dir.open()?;
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
    s.apply(batch, PersistMode::Buffer)?;
    assert_eq!(
        s.get_versionless(b"\xff")?,
        Some(Vec::new()),
        "an empty value is a value, not a removal"
    );
    assert_eq!(s.get_versionless(b"absent")?, None);
    assert_eq!(s.get_versionless(b"\x00\x00")?, None);
    let expected: std::collections::BTreeMap<Vec<u8>, Vec<u8>> = writes
        .iter()
        .map(|(k, v)| (k.to_vec(), v.to_vec()))
        .collect();
    assert_eq!(s.dump_versionless()?, expected);
    drop(s);
    // The keyspace holds each key as it is and its value raw, without the data keyspace's escaping and framing.
    let db = fjall::Database::builder(dir.path()).open()?;
    let terms = db.keyspace(TERMS_KEYSPACE, fjall::KeyspaceCreateOptions::default)?;
    let raw: std::collections::BTreeMap<Vec<u8>, Vec<u8>> =
        fjall::Readable::iter(&db.snapshot(), &terms)
            .map(|g| g.into_inner().map(|(k, v)| (k.to_vec(), v.to_vec())))
            .collect::<Result<_, _>>()?;
    assert_eq!(raw, expected);
    Ok(())
}

#[test]
fn test_within_a_batch_the_later_versionless_write_of_a_key_wins() -> TestResult {
    let dir = TestDir::new("versionless-order");
    let s = dir.open()?;
    s.apply(
        versionless_batch(&[(b"a", Some(b"1"))], 1),
        PersistMode::Buffer,
    )?;
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
    )?;
    assert_eq!(s.get_versionless(b"a")?, Some(b"2".to_vec()));
    assert_eq!(s.get_versionless(b"b")?, None);
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        reopened.dump_versionless()?.into_iter().collect::<Vec<_>>(),
        vec![(b"a".to_vec(), b"2".to_vec())]
    );
    Ok(())
}

#[test]
fn test_an_oversized_key_refuses_the_versionless_writes_with_the_batch() -> TestResult {
    let dir = TestDir::new("versionless-oversized");
    let s = dir.open()?;
    // An oversized versioned key refuses the batch's versionless writes.
    let mut batch = WriteBatch::new();
    batch.put_versionless(b"t".to_vec(), b"1".to_vec());
    batch.push(put(&vec![1; 65_001], 1, b""));
    assert_eq!(
        s.apply(batch, PersistMode::Buffer),
        Err(StoreError::key_too_large(65_001, 65_000, vec![1; 1024]))
    );
    assert_eq!(s.get_versionless(b"t")?, None);
    // An oversized versionless key refuses the batch's versioned entries, by the user key's limit and by the stored
    // key's, which counts a versionless key as a versioned one: 44,000 bytes of `0x00 0x01` are 66,010 once encoded
    // and versioned.
    for (key, error) in [
        (
            vec![2u8; 65_001],
            StoreError::key_too_large(65_001, 65_000, vec![2; 1024]),
        ),
        (
            alternating(44_000),
            StoreError::key_too_large(66_010, 65_535, dense_head()),
        ),
    ] {
        let mut refused_batch = WriteBatch::new();
        refused_batch.push(put(b"fine", 1, b""));
        refused_batch.put_versionless(key, b"v".to_vec());
        refused_batch.set_applied_ts(1);
        assert_eq!(s.apply(refused_batch, PersistMode::Buffer), Err(error));
        assert_eq!(s.get_at(b"fine", 1)?, None);
        assert_eq!(s.applied_ts(), 0);
    }
    assert!(s.dump_versionless()?.is_empty());
    assert!(!s.is_poisoned());
    // The longest versionless keys are written: 65,000 bytes, and the densest key of 65,535 bytes once encoded and
    // versioned.
    let longest = vec![3u8; 65_000];
    let zeros = [alternating(43_682), vec![1, 1]].concat();
    let mut longest_batch = WriteBatch::new();
    longest_batch.put_versionless(longest.clone(), b"long".to_vec());
    longest_batch.put_versionless(zeros.clone(), b"zeros".to_vec());
    s.apply(longest_batch, PersistMode::Buffer)?;
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(reopened.get_versionless(&longest)?, Some(b"long".to_vec()));
    assert_eq!(reopened.get_versionless(&zeros)?, Some(b"zeros".to_vec()));
    Ok(())
}

#[test]
fn test_a_batch_of_versionless_writes_alone_records_its_applied_timestamp() -> TestResult {
    let dir = TestDir::new("versionless-applied");
    let s = dir.open()?;
    let mut batch = WriteBatch::new();
    batch.put_versionless(b"t".to_vec(), b"1".to_vec());
    batch.set_applied_ts(5);
    s.apply(batch, PersistMode::Buffer)?;
    assert_eq!(s.applied_ts(), 5);
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        (reopened.applied_ts(), reopened.get_versionless(b"t")?),
        (5, Some(b"1".to_vec()))
    );
    assert!(reopened.dump()?.is_empty());
    Ok(())
}

#[test]
fn test_a_snapshot_reads_versionless_keys_whatever_its_read_timestamp() -> TestResult {
    // A versionless key carries no version: a snapshot reading below the batch's versioned entries still sees it.
    let dir = TestDir::new("versionless-read-ts");
    let s = dir.open()?;
    let mut batch = WriteBatch::new();
    batch.push(put(b"k", 9, b"nine"));
    batch.put_versionless(b"t".to_vec(), b"1".to_vec());
    s.apply(batch, PersistMode::Buffer)?;
    let snapshot = s.snapshot(1)?;
    assert_eq!(snapshot.get(b"k")?, None);
    assert_eq!(snapshot.get_versionless(b"t")?, Some(b"1".to_vec()));
    Ok(())
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the readers run on threads beside the writer, as the production store's readers do"
)]
fn test_readers_see_a_batchs_versionless_and_versioned_writes_together() -> TestResult {
    // Readers take snapshot after snapshot while one batch writes 10,000 keys of each kind: every snapshot sees the
    // first and the last of both kinds, or none of them.
    use std::sync::atomic::{AtomicBool, Ordering};
    let dir = TestDir::new("versionless-whole");
    let s = std::sync::Arc::new(dir.open()?);
    let done = AtomicBool::new(false);
    let observed: Vec<[bool; 4]> = std::thread::scope(|scope| -> TestResult<Vec<[bool; 4]>> {
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let (s, done) = (s.clone(), &done);
                scope.spawn(move || -> Result<Vec<[bool; 4]>, StoreError> {
                    let mut seen = Vec::new();
                    loop {
                        let finished = done.load(Ordering::Acquire);
                        let snapshot = s.snapshot(10)?;
                        seen.push([
                            snapshot.get(b"k0")?.is_some(),
                            snapshot.get(b"k9999")?.is_some(),
                            snapshot.get_versionless(b"t0")?.is_some(),
                            snapshot.get_versionless(b"t9999")?.is_some(),
                        ]);
                        if finished {
                            return Ok(seen);
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
        // The readers stop on the flag whether or not the batch was written, so a failed write ends the scope.
        let applied = s.apply(batch, PersistMode::Buffer);
        done.store(true, Ordering::Release);
        applied?;
        let mut seen = Vec::new();
        for reader in readers {
            seen.extend(
                reader
                    .join()
                    .map_err(|_| TestError::new("a reader thread panicked"))??,
            );
        }
        Ok(seen)
    })?;
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
    Ok(())
}

#[test]
fn test_an_empty_versionless_key_refuses_its_batch_put_or_removed() -> TestResult {
    // fjall's batches refuse an empty key with a panic; the store refuses it first, with Badger's ErrEmptyKey, and the
    // batch writes nothing.
    let dir = TestDir::new("versionless-empty-key");
    let s = dir.open()?;
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
        assert_eq!(refused, Err(StoreError::empty_key()), "removal: {remove}");
        assert_eq!(
            refused.err().context("the expected error")?.to_string(),
            "Key cannot be empty"
        );
    }
    let temporary = s.create_temporary()?;
    assert_eq!(
        s.apply_temporary(&temporary, empty_key_batch(false)),
        Err(StoreError::empty_key())
    );
    assert!(!s.is_poisoned());
    assert_eq!(s.temporary_snapshot(&temporary)?.get(b"k")?, None);
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        (
            reopened.dump()?,
            reopened.dump_versionless()?.len(),
            reopened.applied_ts()
        ),
        (Vec::new(), 0, 0)
    );
    Ok(())
}

#[test]
fn test_a_cached_versionless_value_gives_way_to_writes_and_a_drop_of_everything() -> TestResult {
    let dir = TestDir::new("versionless-cache");
    let s = dir.open()?;
    let versionless = |store: &FjallStore, key: &[u8], value: Option<&[u8]>| -> TestResult {
        let mut batch = WriteBatch::new();
        match value {
            Some(v) => batch.put_versionless(key.to_vec(), v.to_vec()),
            None => batch.remove_versionless(key.to_vec()),
        }
        store.apply(batch, PersistMode::Buffer)?;
        Ok(())
    };
    versionless(&s, b"t", Some(b"1"))?;
    // The first read fills the cache, the second is answered by it.
    assert_eq!(s.get_versionless(b"t")?, Some(b"1".to_vec()));
    assert_eq!(s.get_versionless(b"t")?, Some(b"1".to_vec()));
    versionless(&s, b"t", Some(b"2"))?;
    assert_eq!(s.get_versionless(b"t")?, Some(b"2".to_vec()));
    versionless(&s, b"t", None)?;
    assert_eq!(s.get_versionless(b"t")?, None);
    // A versioned batch beside a versionless write invalidates it too; a drop of everything clears every cached value,
    // in the open store as after a reopen.
    versionless(&s, b"u", Some(b"3"))?;
    assert_eq!(s.get_versionless(b"u")?, Some(b"3".to_vec()));
    let mut batch = WriteBatch::new();
    batch.push(put(b"k", 1, b""));
    batch.put_versionless(b"u".to_vec(), b"4".to_vec());
    s.apply(batch, PersistMode::Buffer)?;
    assert_eq!(s.get_versionless(b"u")?, Some(b"4".to_vec()));
    s.drop_all(Vec::new())?;
    assert_eq!(s.get_versionless(b"u")?, None);
    Ok(())
}

#[test]
fn test_keys_with_zero_runs_about_the_code_boundaries_keep_byte_order() -> TestResult {
    // Runs of 1, 125, 126, 127, 252 and 253 zero bytes, each ending a key, before a one and before 0xFF, behind a one:
    // the lengths where one code gives way to two and two to three. Badger's order is bytewise.
    let mut keys: Vec<Vec<u8>> = [1usize, 125, 126, 127, 252, 253]
        .iter()
        .flat_map(|&n| {
            let run = [vec![1u8], vec![0; n]].concat();
            [
                run.clone(),
                [run.clone(), vec![1]].concat(),
                [run, vec![0xFF]].concat(),
            ]
        })
        .collect();
    keys.extend([vec![1], vec![1, 1], vec![1, 0xFF], vec![0; 300]]);
    let dir = TestDir::new("run-order");
    let s = dir.open()?;
    for v in [1, 2] {
        write(&s, keys.iter().map(|k| put(k, v, b"")).collect())?;
    }
    keys.sort();
    let all: Vec<(Vec<u8>, u64)> = keys
        .iter()
        .flat_map(|k| [(k.clone(), 2), (k.clone(), 1)])
        .collect();
    assert_eq!(
        scanned(&s, &Scan::prefix(Vec::new()).all_versions(), 2)?,
        all
    );
    // A prefix ending in zero bytes selects exactly the keys beginning with it, and a drop of it removes only them.
    let prefix = [vec![1u8], vec![0; 126]].concat();
    let under: Vec<Vec<u8>> = keys
        .iter()
        .filter(|k| k.starts_with(&prefix))
        .cloned()
        .collect();
    let selected: Vec<Vec<u8>> = scanned(&s, &Scan::prefix(prefix.clone()), 2)?
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(selected, under);
    s.drop_prefixes(3, std::slice::from_ref(&prefix), Vec::new())?;
    let left: Vec<Vec<u8>> = scanned(&s, &Scan::prefix(Vec::new()), 3)?
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    let kept: Vec<Vec<u8>> = keys
        .into_iter()
        .filter(|k| !k.starts_with(&prefix))
        .collect();
    assert_eq!(left, kept);
    Ok(())
}
