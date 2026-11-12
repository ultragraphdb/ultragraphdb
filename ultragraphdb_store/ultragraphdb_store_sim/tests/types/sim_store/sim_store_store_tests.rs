/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store's contract (the kv-store specification) as the simulated store keeps it.

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_sim::{Contents, FaultConfig, OpRecord, Seed, SimClock, SimStore};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{
    Entry, Item, PersistMode, RebuildRecord, Scan, Store, StoreSnapshot, WriteBatch,
};

fn store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

fn write(store: &SimStore, entries: Vec<Entry>) -> TestResult {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store
        .apply(batch, PersistMode::Buffer)
        .context("no fault is configured")?;
    Ok(())
}

fn put(key: &[u8], version: u64, value: &[u8]) -> Entry {
    Entry::put(key.to_vec(), version, 0x04, value.to_vec())
}

/// A key of tenant `tenant`.
fn tenant_key(tenant: u64, tail: &[u8]) -> Vec<u8> {
    let mut key = vec![0x00];
    key.extend_from_slice(&tenant.to_be_bytes());
    key.extend_from_slice(tail);
    key
}

fn versions(store: &SimStore, key: &[u8], ts: u64) -> TestResult<Vec<u64>> {
    let snapshot = store.snapshot(ts).context("readable")?;
    snapshot
        .versions(key)
        .map(|i| Ok(i.context("no fault")?.version()))
        .collect()
}

fn scanned(store: &SimStore, scan: &Scan, ts: u64) -> TestResult<Vec<(Vec<u8>, u64)>> {
    let snapshot = store.snapshot(ts).context("readable")?;
    snapshot
        .scan(scan)
        .map(|i| i.context("no fault"))
        .map(|i| i.map(|item| (item.key().to_vec(), item.version())))
        .collect()
}

#[test]
fn test_a_keys_versions_iterate_newest_first_with_their_meta() -> TestResult {
    let s = store();
    for v in [3u8, 7, 5] {
        write(&s, vec![Entry::put(b"k".to_vec(), u64::from(v), v, vec![])])?;
    }
    let snapshot = s.snapshot(10)?;
    let items: Vec<(u64, u8)> = snapshot
        .versions(b"k")
        .map(|i| i.map(|item| (item.version(), item.meta())))
        .collect::<Result<_, _>>()?;
    assert_eq!(items, vec![(7, 7), (5, 5), (3, 3)]);
    Ok(())
}

#[test]
fn test_a_version_below_the_newest_can_be_written() -> TestResult {
    let s = store();
    write(&s, vec![put(b"k", 9, b"a")])?;
    write(&s, vec![put(b"k", 6, b"b")])?;
    assert_eq!(versions(&s, b"k", 10)?, vec![9, 6]);
    Ok(())
}

#[test]
fn test_rewriting_a_version_replaces_it() -> TestResult {
    let s = store();
    write(&s, vec![put(b"k", 4, b"first")])?;
    write(&s, vec![put(b"k", 4, b"second")])?;
    assert_eq!(s.get_at(b"k", 4)?.context("a value")?.value(), b"second");
    assert_eq!(versions(&s, b"k", 4)?, vec![4]);
    Ok(())
}

#[test]
fn test_a_key_that_prefixes_another_sorts_first() -> TestResult {
    let s = store();
    for v in [3, 5] {
        write(&s, vec![put(b"\x02ab", v, b""), put(b"\x02abc", v, b"")])?;
    }
    let scan = Scan::prefix(b"\x02a".to_vec()).all_versions();
    let keys = scanned(&s, &scan, 9)?;
    let expected: Vec<(Vec<u8>, u64)> = vec![
        (b"\x02ab".to_vec(), 5),
        (b"\x02ab".to_vec(), 3),
        (b"\x02abc".to_vec(), 5),
        (b"\x02abc".to_vec(), 3),
    ];
    assert_eq!(keys, expected);
    Ok(())
}

#[test]
fn test_an_oversized_key_refuses_its_whole_batch() -> TestResult {
    let s = store();
    let mut batch = WriteBatch::new();
    batch.push(put(b"fine", 1, b""));
    batch.push(put(&vec![1; 65_001], 1, b""));
    let err = s
        .apply(batch, PersistMode::Buffer)
        .err()
        .context("an error")?;
    assert!(
        err.to_string()
            .starts_with("Key with size 65001 exceeded 65000 limit")
    );
    assert_eq!(s.get_at(b"fine", 1)?, None);

    let mut encoded = WriteBatch::new();
    encoded.push(put(b"fine", 1, b""));
    // 44,000 bytes of `0x00 0x01`: every zero byte a run of one, 66,010 bytes once encoded and versioned.
    encoded.push(put(
        &(0..44_000)
            .map(|i| u8::from(i % 2 == 1))
            .collect::<Vec<u8>>(),
        1,
        b"",
    ));
    let encoded_err = s
        .apply(encoded, PersistMode::Buffer)
        .err()
        .context("an error")?;
    assert!(
        encoded_err
            .to_string()
            .starts_with("Key with size 66010 exceeded 65535 limit")
    );
    assert_eq!(s.get_at(b"fine", 1)?, None);
    assert!(
        !s.needs_recovery(),
        "a refused batch does not poison the store"
    );
    Ok(())
}

#[test]
fn test_a_read_takes_the_newest_version_at_or_below_its_timestamp() -> TestResult {
    let s = store();
    write(&s, vec![put(b"k", 3, b"three"), put(b"k", 7, b"seven")])?;
    assert_eq!(s.get_at(b"k", 5)?.context("a value")?.version(), 3);
    assert_eq!(s.get_at(b"k", 7)?.context("a value")?.value(), b"seven");
    assert_eq!(s.get_at(b"k", 2)?, None);
    Ok(())
}

#[test]
fn test_a_deletion_hides_the_key() -> TestResult {
    let s = store();
    write(
        &s,
        vec![put(b"k", 3, b"v"), Entry::deletion(b"k".to_vec(), 6)],
    )?;
    assert_eq!(s.get_at(b"k", 7)?, None);
    assert_eq!(s.get_at(b"k", 4)?.context("a value")?.version(), 3);
    assert_eq!(versions(&s, b"k", 7)?, Vec::<u64>::new());
    assert!(scanned(&s, &Scan::prefix(b"k".to_vec()), 7)?.is_empty());
    Ok(())
}

#[test]
fn test_a_read_below_the_discard_timestamp_is_refused() -> TestResult {
    let s = store();
    write(&s, vec![put(b"k", 3, b"v")])?;
    s.set_discard_ts(10)?;
    for below in [0, 5, 9] {
        let refused = s.get_at(b"k", below).err().context("an error")?;
        assert_eq!(refused, StoreError::ts_too_old());
        assert_eq!(refused.to_string(), "Transaction is too old");
    }
    let at = s
        .get_at(b"k", 10)?
        .context("a read at the discard timestamp")?;
    assert_eq!((at.version(), at.value()), (3, &b"v"[..]));
    Ok(())
}

#[test]
fn test_a_scan_is_unaffected_by_a_later_batch() -> TestResult {
    let s = store();
    write(&s, vec![put(b"p1", 1, b"")])?;
    let snapshot = s.snapshot(100)?;
    write(&s, vec![put(b"p2", 2, b""), put(b"p1", 3, b"")])?;
    let seen: Vec<Item> = snapshot
        .scan(&Scan::prefix(b"p".to_vec()))
        .map(Result::unwrap)
        .collect();
    assert_eq!(seen.len(), 1);
    assert_eq!((seen[0].key(), seen[0].version()), (&b"p1"[..], 1));
    Ok(())
}

#[test]
fn test_a_scan_reports_each_key_once_with_its_newest_version() -> TestResult {
    let s = store();
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
    Ok(())
}

#[test]
fn test_a_reverse_scan_descends_with_versions_newest_first() -> TestResult {
    let s = store();
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
    Ok(())
}

#[test]
fn test_a_batch_holds_entries_at_several_versions() -> TestResult {
    let s = store();
    write(&s, vec![put(b"a", 12, b""), put(b"b", 13, b"")])?;
    let held = |ts| -> TestResult<Vec<Option<u64>>> {
        Ok(s.snapshot(ts)?
            .get_many(&[b"a", b"b"])?
            .iter()
            .map(|i| i.as_ref().map(Item::version))
            .collect())
    };
    assert_eq!(held(11)?, [None, None]);
    assert_eq!(held(12)?, [Some(12), None]);
    assert_eq!(held(13)?, [Some(12), Some(13)]);
    Ok(())
}

#[test]
fn test_a_batch_records_the_highest_applied_timestamp() -> TestResult {
    let s = store();
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
    Ok(())
}

#[test]
fn test_a_dropped_prefix_is_gone_after_reopening_and_below_the_drop() -> TestResult {
    let s = store();
    write(
        &s,
        vec![
            put(b"\x00a1", 3, b""),
            put(b"\x00a2", 3, b""),
            put(b"\x00b1", 3, b""),
        ],
    )?;
    s.drop_prefixes(8, &[b"\x00a".to_vec()], vec![put(b"\x00a9", 1, b"initial")])?;
    s.crash();
    s.recover();
    assert_eq!(
        s.get_at(b"\x00a1", 5)?,
        None,
        "a read below the drop finds nothing of it"
    );
    assert_eq!(s.get_at(b"\x00a2", 9)?, None);
    assert_eq!(s.get_at(b"\x00b1", 9)?.context("a value")?.version(), 3);
    assert_eq!(
        s.get_at(b"\x00a9", 9)?.context("a value")?.value(),
        b"initial"
    );
    Ok(())
}

#[test]
fn test_dropping_everything_keeps_only_its_entries_and_clears_the_metadata() -> TestResult {
    let s = store();
    let mut batch = WriteBatch::new();
    batch.push(put(b"old", 3, b""));
    batch.set_applied_ts(3);
    s.apply(batch, PersistMode::Buffer)?;
    s.set_discard_ts(2)?;
    s.set_tombstone_horizon(1)?;
    s.drop_all(vec![put(b"\x01schema", 1, b"s")])?;
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
    Ok(())
}

#[test]
fn test_a_banned_tenant_ignores_writes_and_hides_its_keys() -> TestResult {
    let s = store();
    let (five, six) = (tenant_key(5, b"\x00\x01a"), tenant_key(6, b"\x00\x01a"));
    write(&s, vec![put(&five, 1, b"before")])?;
    s.ban_tenant(5)?;
    write(&s, vec![put(&five, 2, b""), put(&six, 2, b"")])?;
    assert_eq!(s.get_at(&five, 3)?, None);
    assert_eq!(s.get_at(&six, 3)?.map(|i| i.version()), Some(2));
    assert!(
        scanned(&s, &Scan::prefix(vec![0x00]), 3)?
            .iter()
            .all(|(k, _)| *k != five)
    );
    assert_eq!(
        s.contents().versions(&five).len(),
        1,
        "the ban hides the earlier version, it does not remove it"
    );
    Ok(())
}

#[test]
fn test_dropping_everything_clears_the_banned_set() -> TestResult {
    let s = store();
    let five = tenant_key(5, b"\x00\x01a");
    s.ban_tenant(5)?;
    s.drop_all(Vec::new())?;
    write(&s, vec![put(&five, 1, b"after")])?;
    assert_eq!(s.contents().banned().len(), 0);
    assert_eq!(
        s.get_at(&five, 1)?.map(|i| i.value().to_vec()),
        Some(b"after".to_vec())
    );
    Ok(())
}

#[test]
fn test_a_poisoned_store_refuses_every_write_until_recovered() -> TestResult {
    let s = store();
    s.inject(ultragraphdb_store_sim::Fault::FailNextBatch);
    let failed = s
        .apply(WriteBatch::new(), PersistMode::Buffer)
        .err()
        .context("an error")?;
    assert_eq!(
        failed.to_string(),
        "Write to the store failed: simulated failure during the batch's persist"
    );
    assert!(matches!(failed.0, StoreErrorEnum::WriteFailed(_)));
    let poisoned = Err(StoreError::poisoned());
    let temporary = ultragraphdb_traits_store::Temporary::new(1);
    assert_eq!(s.apply(WriteBatch::new(), PersistMode::Buffer), poisoned);
    assert_eq!(s.sync(), poisoned);
    assert_eq!(s.compact(), poisoned);
    assert_eq!(s.sweep_deletions(), poisoned);
    assert_eq!(s.ban_tenant(1), poisoned);
    assert_eq!(s.set_discard_ts(1), poisoned);
    assert_eq!(s.set_tombstone_horizon(1), poisoned);
    assert_eq!(s.drop_all(Vec::new()), poisoned);
    assert_eq!(s.drop_prefixes(1, &[], Vec::new()), poisoned);
    assert_eq!(s.create_temporary().err(), Some(StoreError::poisoned()));
    assert_eq!(s.apply_temporary(&temporary, WriteBatch::new()), poisoned);
    assert_eq!(s.drop_temporary(temporary), poisoned);
    assert_eq!(
        s.write_rebuild_record(&RebuildRecord::new(1, Vec::new())),
        poisoned
    );
    assert_eq!(
        s.snapshot(0)?.read_ts(),
        0,
        "a poisoned process still reads"
    );
    assert_eq!(s.rebuild_records(), Ok(Vec::new()));
    let refused: Vec<&'static str> = s
        .take_log()
        .iter()
        .filter_map(|r| {
            if let OpRecord::WriteWhileFailed { operation } = r {
                Some(*operation)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        refused,
        [
            "apply",
            "sync",
            "compact",
            "sweep_deletions",
            "ban_tenant",
            "set_discard_ts",
            "set_tombstone_horizon",
            "drop",
            "drop",
            "create_temporary",
            "apply_temporary",
            "drop_temporary",
            "write_rebuild_record",
        ]
    );
    s.recover();
    assert_eq!(s.apply(WriteBatch::new(), PersistMode::Buffer), Ok(()));
    assert!(!s.needs_recovery());
    Ok(())
}

#[test]
fn test_an_oversized_key_refuses_a_drop_before_it_removes_anything() -> TestResult {
    let s = store();
    write(&s, vec![put(b"\x00a", 1, b"")])?;
    let refused = s
        .drop_prefixes(2, &[b"\x00".to_vec()], vec![put(&vec![1; 65_001], 1, b"")])
        .err()
        .context("an error")?;
    assert!(matches!(
        refused.0,
        StoreErrorEnum::KeyTooLarge {
            size: 65_001,
            limit: 65_000,
            ..
        }
    ));
    assert_eq!(
        s.drop_all(vec![put(&vec![1; 65_001], 1, b"")])
            .err()
            .context("an error")?,
        refused
    );
    assert_eq!(s.contents().versions(b"\x00a").len(), 1);
    assert!(!s.needs_recovery());
    assert!(matches!(
        s.take_log().last(),
        Some(OpRecord::Refused { error, .. }) if *error == refused
    ));
    Ok(())
}

#[test]
fn test_a_drop_removes_every_key_under_any_of_its_prefixes_and_no_other() -> TestResult {
    let s = store();
    // 130 keys under one prefix take three removal batches.
    let under: Vec<Vec<u8>> = (0..130u8).map(|i| vec![0x02, b'a', i]).collect();
    for k in &under {
        write(&s, vec![put(k, 1, b""), put(k, 2, b"")])?;
    }
    for key in [&b"\x02b1"[..], b"\x02c", b"\x03a", b"\x02"] {
        write(&s, vec![put(key, 1, b"")])?;
    }
    s.drop_prefixes(9, &[b"\x02a".to_vec(), b"\x02b".to_vec()], Vec::new())?;
    let left: Vec<Vec<u8>> = s.contents().keys().keys().cloned().collect();
    assert_eq!(
        left,
        [b"\x02".to_vec(), b"\x02c".to_vec(), b"\x03a".to_vec()]
    );
    Ok(())
}

#[test]
fn test_a_drop_without_prefixes_removes_nothing_and_writes_its_entries() -> TestResult {
    let s = store();
    write(&s, vec![put(b"\x00a", 1, b"")])?;
    s.drop_prefixes(2, &[], vec![put(b"\x00b", 1, b"e")])?;
    assert_eq!(
        s.contents().keys().keys().cloned().collect::<Vec<_>>(),
        [b"\x00a".to_vec(), b"\x00b".to_vec()]
    );
    Ok(())
}

/// A batch of versionless writes, with one versioned entry beside them.
fn versionless_batch(writes: &[(&[u8], Option<&[u8]>)], version: u64) -> WriteBatch {
    let mut batch = WriteBatch::new();
    batch.push(put(&tenant_key(0, b"data"), version, b"v"));
    for (key, value) in writes {
        match value {
            Some(value) => batch.put_versionless(key.to_vec(), value.to_vec()),
            None => batch.remove_versionless(key.to_vec()),
        }
    }
    batch
}

#[test]
fn test_versionless_writes_replace_remove_and_survive_a_crash() -> TestResult {
    let s = store();
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
    s.crash();
    assert_eq!(
        s.get_versionless(b"a"),
        Err(StoreError::poisoned()),
        "a process that is down reads nothing"
    );
    s.recover();
    let held: Vec<_> = s.contents().versionless().clone().into_iter().collect();
    assert_eq!(held, vec![(b"a".to_vec(), b"3".to_vec())]);
    Ok(())
}

#[test]
fn test_a_snapshot_reads_the_versionless_keys_of_its_opening() -> TestResult {
    let s = store();
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
    drop(snapshot);
    s.set_discard_ts(2)?;
    assert!(matches!(s.snapshot(1), Err(e) if matches!(e.0, StoreErrorEnum::TsTooOld)));
    assert_eq!(s.get_versionless(b"b")?, Some(b"3".to_vec()));
    Ok(())
}

#[test]
fn test_bans_prefix_drops_and_temporary_keyspaces_leave_versionless_keys_alone() -> TestResult {
    let s = store();
    s.apply(
        versionless_batch(&[(&tenant_key(5, b"t"), Some(b"1"))], 1),
        PersistMode::Buffer,
    )?;
    s.ban_tenant(5)?;
    s.apply(
        versionless_batch(&[(&tenant_key(5, b"u"), Some(b"2"))], 2),
        PersistMode::Buffer,
    )?;
    s.drop_prefixes(3, &[vec![0x00]], Vec::new())?;
    assert_eq!(
        s.get_versionless(&tenant_key(5, b"t"))?,
        Some(b"1".to_vec())
    );
    assert_eq!(
        s.get_versionless(&tenant_key(5, b"u"))?,
        Some(b"2".to_vec())
    );
    let temporary = s.create_temporary()?;
    let mut batch = WriteBatch::new();
    batch.put_versionless(b"tmp".to_vec(), b"x".to_vec());
    s.apply_temporary(&temporary, batch)?;
    let snapshot = s.temporary_snapshot(&temporary)?;
    assert_eq!(snapshot.get_versionless(&tenant_key(5, b"t"))?, None);
    assert_eq!(
        s.get_versionless(b"tmp")?,
        None,
        "a temporary batch's versionless writes are ignored"
    );
    s.drop_all(Vec::new())?;
    assert!(
        s.contents().versionless().is_empty(),
        "a drop of everything clears them"
    );
    Ok(())
}

#[test]
fn test_a_versionless_read_is_logged_with_its_snapshot() -> TestResult {
    let s = store();
    s.apply(
        versionless_batch(&[(b"a", Some(b"1"))], 1),
        PersistMode::Buffer,
    )?;
    let snapshot = s.snapshot(1)?;
    let _earlier: Vec<OpRecord> = s.take_log();
    assert_eq!(snapshot.get_versionless(b"a")?, Some(b"1".to_vec()));
    assert_eq!(s.get_versionless(b"z")?, None);
    let id = snapshot.id();
    assert_eq!(
        s.take_log(),
        vec![
            OpRecord::ReadVersionless {
                id: Some(id),
                key: b"a".to_vec(),
                result: Ok(Some(b"1".to_vec()))
            },
            OpRecord::ReadVersionless {
                id: None,
                key: b"z".to_vec(),
                result: Ok(None)
            },
        ]
    );
    Ok(())
}

/// A versionless key over the size limit refuses its batch whole, its entries with it, and the refusal names the
/// key's size and the limit: 65 001 bytes against `MAX_USER_KEY_SIZE`, 65 000.
#[test]
fn test_an_oversized_versionless_key_refuses_its_batch() -> TestResult {
    let s = store();
    let mut batch = versionless_batch(&[(b"fine", Some(b"1"))], 1);
    batch.put_versionless(vec![1; 65_001], b"2".to_vec());
    let refused = s
        .apply(batch, PersistMode::Buffer)
        .err()
        .context("an error")?;
    assert!(
        matches!(
            refused.0,
            StoreErrorEnum::KeyTooLarge {
                size: 65_001,
                limit: 65_000,
                ..
            }
        ),
        "{refused:?}"
    );
    assert_eq!(s.contents(), Contents::default(), "nothing was written");
    assert!(!s.needs_recovery(), "a refusal poisons nothing");
    Ok(())
}
