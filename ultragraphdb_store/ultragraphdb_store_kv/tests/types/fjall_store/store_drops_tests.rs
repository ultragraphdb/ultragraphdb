/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Drops through their record: a record left by a crash is finished before the store serves.

use fjall::{Database, KeyspaceCreateOptions};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_kv::{DropStage, META_KEYSPACE};
use ultragraphdb_traits_store::{Entry, Store};

use crate::test_store::{TestDir, held, ns_key, put, write};

/// An entry of a drop record: key, version, kind, meta byte and value.
type RecordEntry<'a> = (&'a [u8], u64, u8, u8, &'a [u8]);

/// A drop record as the store writes it: kind, timestamp, prefixes, then entries with key, version, kind, meta and
/// value, big-endian with four-byte counts and lengths. Every entry is a put.
fn record(prefixes: Option<&[&[u8]]>, ts: u64, entries: &[(&[u8], u64, u8, &[u8])]) -> Vec<u8> {
    let puts: Vec<RecordEntry> = entries
        .iter()
        .map(|&(key, version, meta, value)| (key, version, 0x01, meta, value))
        .collect();
    record_of(prefixes, ts, &puts)
}

/// [`record`] with each entry's kind byte given: 0 a deletion, 1 a put, 2 a put with the discard flag.
fn record_of(prefixes: Option<&[&[u8]]>, ts: u64, entries: &[RecordEntry]) -> Vec<u8> {
    let mut out = vec![u8::from(prefixes.is_none())];
    out.extend_from_slice(&ts.to_be_bytes());
    let len = |out: &mut Vec<u8>, n: usize| out.extend_from_slice(&(n as u32).to_be_bytes());
    let ps = prefixes.unwrap_or_default();
    len(&mut out, ps.len());
    for p in ps {
        len(&mut out, p.len());
        out.extend_from_slice(p);
    }
    len(&mut out, entries.len());
    for (key, version, kind, meta, value) in entries {
        len(&mut out, key.len());
        out.extend_from_slice(key);
        out.extend_from_slice(&version.to_be_bytes());
        out.push(*kind);
        out.push(*meta);
        len(&mut out, value.len());
        out.extend_from_slice(value);
    }
    out
}

fn leave_record(dir: &TestDir, bytes: Vec<u8>) {
    let db = Database::builder(dir.path()).open().unwrap();
    let meta = db
        .keyspace(META_KEYSPACE, KeyspaceCreateOptions::default)
        .unwrap();
    meta.insert(b"drop", bytes).unwrap();
    db.persist(fjall::PersistMode::SyncAll).unwrap();
}

#[test]
fn test_a_prefix_drop_left_by_a_crash_is_finished_at_open() {
    let dir = TestDir::new("drop-record");
    let s = dir.open();
    write(
        &s,
        vec![
            put(b"\x00a1", 3, b""),
            put(b"\x00a2", 4, b""),
            put(b"\x00b1", 3, b""),
        ],
    );
    drop(s);
    leave_record(
        &dir,
        record(Some(&[b"\x00a"]), 8, &[(b"\x00a9", 1, 0x01, b"initial")]),
    );
    let s = dir.open();
    assert!(held(&s, b"\x00a1").is_empty() && held(&s, b"\x00a2").is_empty());
    assert_eq!(held(&s, b"\x00b1"), vec![(3, false)]);
    assert_eq!(s.get_at(b"\x00a9", 2).unwrap().unwrap().value(), b"initial");
    // Written again, the key under the prefix would be removed by a drop repeated at the next open.
    write(&s, vec![put(b"\x00a3", 9, b"after")]);
    drop(s);
    let s = dir.open();
    assert_eq!(
        s.dump().unwrap(),
        vec![
            put(b"\x00a3", 9, b"after"),
            Entry::put(b"\x00a9".to_vec(), 1, 0x01, b"initial".to_vec()),
            put(b"\x00b1", 3, b""),
        ],
        "the finished drop deleted its record and is not repeated"
    );
}

#[test]
fn test_a_drop_of_everything_left_by_a_crash_is_finished_at_open() {
    let dir = TestDir::new("drop-all-record");
    let s = dir.open();
    let mut batch = ultragraphdb_traits_store::WriteBatch::new();
    batch.push(put(b"old", 3, b""));
    batch.set_applied_ts(3);
    s.apply(batch, ultragraphdb_traits_store::PersistMode::Buffer)
        .unwrap();
    s.ban_namespace(5).unwrap();
    s.set_discard_ts(2).unwrap();
    drop(s);
    let five = ns_key(5, b"\x00\x01a");
    leave_record(&dir, record(None, 0, &[(&five, 1, 0x01, b"s")]));
    let s = dir.open();
    let keys: Vec<Vec<u8>> = s.dump().unwrap().iter().map(|e| e.key().to_vec()).collect();
    assert_eq!(
        keys,
        vec![five],
        "the clear lifts the ban before the entries are written"
    );
    assert_eq!((s.applied_ts(), s.discard_ts()), (0, 0));
}

#[test]
fn test_an_unreadable_drop_record_refuses_the_open() {
    let dir = TestDir::new("drop-record-bad");
    drop(dir.open());
    leave_record(&dir, vec![7]);
    let err = ultragraphdb_store_kv::FjallStore::open(
        dir.path(),
        ultragraphdb_store_kv::Durability::Process,
    )
    .unwrap_err();
    assert_eq!(
        err,
        StoreError::Corrupted(b"drop".to_vec(), "unknown drop kind 7".to_string())
    );
    assert_eq!(
        err.to_string(),
        "Corrupted value of key 64726f70: unknown drop kind 7"
    );
    // 1 + 8 + 4 + (4 + 1) + 4 bytes; the entry count loses its last byte.
    let mut short = record(Some(&[b"p"]), 1, &[]);
    short.pop();
    let mut trailing = record(None, 0, &[]);
    trailing.push(0xAA);
    let unknown_entry = record_of(None, 0, &[(b"k", 1, 9, 0x01, b"v")]);
    for (bytes, reason) in [
        (Vec::new(), "drop record ends after 0 of 1 bytes"),
        (vec![0x00, 0x01], "drop record ends after 1 of 8 bytes"),
        (short, "drop record ends after 3 of 4 bytes"),
        (trailing, "1 bytes after the drop record"),
        (unknown_entry, "unknown entry kind 9"),
    ] {
        leave_record(&dir, bytes);
        assert_eq!(
            ultragraphdb_store_kv::FjallStore::open(
                dir.path(),
                ultragraphdb_store_kv::Durability::Process
            )
            .unwrap_err(),
            StoreError::Corrupted(b"drop".to_vec(), reason.to_string()),
            "{reason}"
        );
    }
}

#[test]
fn test_a_drop_record_carries_deletions_and_discard_flags() {
    let dir = TestDir::new("drop-record-kinds");
    let s = dir.open();
    write(&s, vec![put(b"\x00a1", 3, b"")]);
    drop(s);
    leave_record(
        &dir,
        record_of(
            None,
            0,
            &[
                (b"\x01d", 2, 0x00, 0x00, b""),
                (b"\x01p", 1, 0x01, 0x04, b"put"),
                (b"\x01q", 1, 0x02, 0x08, b"discard"),
            ],
        ),
    );
    let s = dir.open();
    assert_eq!(
        s.dump().unwrap(),
        vec![
            Entry::deletion(b"\x01d".to_vec(), 2),
            Entry::put(b"\x01p".to_vec(), 1, 0x04, b"put".to_vec()),
            Entry::put(b"\x01q".to_vec(), 1, 0x08, b"discard".to_vec()).with_discard(),
        ]
    );
}

#[test]
fn test_a_drop_refuses_an_oversized_entry_and_drops_nothing() {
    let dir = TestDir::new("drop-oversized");
    let s = dir.open();
    write(&s, vec![put(b"\x00a1", 3, b"")]);
    let err = s
        .drop_prefixes(8, &[b"\x00a".to_vec()], vec![put(&vec![1; 65_001], 1, b"")])
        .unwrap_err();
    // Badger's exceedsSize dumps the first 1 << 10 bytes of the user key.
    assert_eq!(err, StoreError::KeyTooLarge(65_001, 65_000, vec![1; 1024]));
    assert!(
        err.to_string()
            .starts_with("Key with size 65001 exceeded 65000 limit. Key:\n"),
        "{err}"
    );
    assert_eq!(held(&s, b"\x00a1"), vec![(3, false)]);
    let err = s
        .drop_all(vec![Entry::deletion(vec![0; 40_000], 1)])
        .unwrap_err();
    // 40,000 zero bytes escaped to 80,000, the terminator's 2 and the version's 8; the head dumps the escaped bytes.
    assert_eq!(
        err.0,
        StoreErrorEnum::KeyTooLarge {
            size: 80_010,
            limit: 65_535,
            head: [0x00, 0xFF].repeat(512),
        }
    );
    assert_eq!(held(&s, b"\x00a1"), vec![(3, false)]);
    drop(s);
    let s = dir.open();
    assert_eq!(
        held(&s, b"\x00a1"),
        vec![(3, false)],
        "the refused drops left no record to finish at open"
    );
}

#[test]
fn test_a_drop_removes_every_version_in_as_many_batches_as_it_needs() {
    let dir = TestDir::new("drop-many");
    let s = dir.open();
    let entries: Vec<Entry> = (0..25_000u32)
        .map(|i| put(&[b"\x00z".as_slice(), &i.to_be_bytes()].concat(), 2, b""))
        .collect();
    write(&s, entries);
    s.drop_prefixes(9, &[b"\x00z".to_vec(), b"\x00z\x00".to_vec()], Vec::new())
        .unwrap();
    assert!(s.dump().unwrap().is_empty());
}

#[test]
fn test_an_observed_drop_reports_each_stage_it_reaches_in_order() {
    let dir = TestDir::new("drop-stages");
    let s = dir.open();
    let entries: Vec<Entry> = (0..25_000u32)
        .map(|i| put(&[b"\x00z".as_slice(), &i.to_be_bytes()].concat(), 2, b""))
        .collect();
    write(&s, entries);
    let mut stages = Vec::new();
    s.drop_prefixes_observed(
        9,
        &[b"\x00z".to_vec()],
        vec![put(b"\x00z-initial", 1, b"")],
        |stage| stages.push(stage),
    )
    .unwrap();
    assert_eq!(
        stages,
        vec![
            DropStage::RecordSynced,
            DropStage::Removing(10_000),
            DropStage::Removing(20_000),
            DropStage::DataRemoved,
            DropStage::EntriesSynced,
        ]
    );
    assert_eq!(held(&s, b"\x00z-initial"), vec![(1, false)]);
}

/// The user keys the store holds, in order.
fn keys(s: &ultragraphdb_store_kv::FjallStore) -> Vec<Vec<u8>> {
    let mut keys: Vec<Vec<u8>> = s.dump().unwrap().iter().map(|e| e.key().to_vec()).collect();
    keys.dedup();
    keys
}

#[test]
fn test_a_prefix_drop_removes_the_prefix_itself_and_nothing_beside_it() {
    let dir = TestDir::new("drop-bounds");
    let s = dir.open();
    let all: [&[u8]; 9] = [
        b"",
        b"\x00",
        b"\x00`",
        b"\x00a",
        b"\x00a\x00",
        b"\x00a\xff",
        b"\x00ab",
        b"\x00b",
        b"a",
    ];
    write(&s, all.iter().map(|k| put(k, 3, b"")).collect());
    s.drop_prefixes(8, &[b"\x00a".to_vec()], Vec::new())
        .unwrap();
    // The prefix is a key of its own and begins every key it covers; 0x60 and 0x62 sort on either side of 0x61.
    assert_eq!(
        keys(&s),
        vec![
            b"".to_vec(),
            b"\x00".to_vec(),
            b"\x00`".to_vec(),
            b"\x00b".to_vec(),
            b"a".to_vec()
        ]
    );
    // The zero byte alone covers every key beginning with it, and not the empty key.
    s.drop_prefixes(9, &[vec![0x00]], Vec::new()).unwrap();
    assert_eq!(keys(&s), vec![b"".to_vec(), b"a".to_vec()]);
    s.drop_prefixes(10, &[vec![0xFF]], Vec::new()).unwrap();
    assert_eq!(
        keys(&s),
        vec![b"".to_vec(), b"a".to_vec()],
        "a prefix no key has"
    );
    drop(s);
    assert_eq!(keys(&dir.open()), vec![b"".to_vec(), b"a".to_vec()]);
}

#[test]
fn test_the_empty_prefix_drops_every_version_and_keeps_the_metadata() {
    // A prefix drop leaves the applied timestamp, the discard timestamp, bans and the keys without versions alone;
    // only a drop of everything clears them.
    let dir = TestDir::new("drop-empty-prefix");
    let s = dir.open();
    let mut batch = ultragraphdb_traits_store::WriteBatch::new();
    batch.push(put(b"", 3, b""));
    batch.push(put(b"\xff\xff", 3, b""));
    batch.put_versionless(b"t".to_vec(), b"1".to_vec());
    batch.set_applied_ts(3);
    s.apply(batch, ultragraphdb_traits_store::PersistMode::Buffer)
        .unwrap();
    s.set_discard_ts(2).unwrap();
    s.ban_namespace(5).unwrap();
    s.drop_prefixes(4, &[Vec::new()], vec![put(b"\x01catalog", 1, b"c")])
        .unwrap();
    drop(s);
    let s = dir.open();
    assert_eq!(s.dump().unwrap(), vec![put(b"\x01catalog", 1, b"c")]);
    assert_eq!((s.applied_ts(), s.discard_ts()), (3, 2));
    assert_eq!(s.get_versionless(b"t").unwrap(), Some(b"1".to_vec()));
    write(&s, vec![put(&ns_key(5, b"x"), 4, b"")]);
    assert_eq!(
        s.get_at(&ns_key(5, b"x"), 4).unwrap(),
        None,
        "the ban holds"
    );
}

#[test]
fn test_a_drop_writes_no_entry_addressed_to_a_banned_namespace() {
    let dir = TestDir::new("drop-banned-entry");
    let s = dir.open();
    s.ban_namespace(5).unwrap();
    s.drop_prefixes(
        2,
        &[vec![0x00]],
        vec![
            put(&ns_key(5, b"x"), 1, b"banned"),
            put(&ns_key(6, b"x"), 1, b"kept"),
        ],
    )
    .unwrap();
    assert_eq!(s.dump().unwrap(), vec![put(&ns_key(6, b"x"), 1, b"kept")]);
}

#[test]
fn test_only_a_prefix_drop_fences_its_entries() {
    // A drop of everything clears the data, so its entries need no fence and garbage collection takes them as any
    // other version; a prefix drop's entries are never collected.
    let dir = TestDir::new("drop-fence-kinds");
    let s = dir.open();
    s.drop_all(vec![put(b"\x01all", 1, b"")]).unwrap();
    s.drop_prefixes(2, &[b"\x02".to_vec()], vec![put(b"\x02prefix", 1, b"")])
        .unwrap();
    write(
        &s,
        vec![
            put(b"\x01all", 5, b"").with_discard(),
            put(b"\x02prefix", 5, b"").with_discard(),
        ],
    );
    s.set_discard_ts(6).unwrap();
    s.compact().unwrap();
    assert_eq!(held(&s, b"\x01all"), vec![(5, false)]);
    assert_eq!(held(&s, b"\x02prefix"), vec![(5, false), (1, false)]);
}
