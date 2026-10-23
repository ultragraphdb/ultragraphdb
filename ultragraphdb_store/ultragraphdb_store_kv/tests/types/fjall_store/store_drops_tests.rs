/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Drops through their record: a record left by a crash is finished before the store serves.

use fjall::{Database, KeyspaceCreateOptions};
use ultragraphdb_store_kv::META_KEYSPACE;
use ultragraphdb_traits_store::{Entry, Store};

use crate::test_store::{TestDir, held, ns_key, put, write};

/// A drop record as the store writes it: kind, timestamp, prefixes, then entries with key, version, kind, meta and
/// value, big-endian with four-byte counts and lengths.
fn record(prefixes: Option<&[&[u8]]>, ts: u64, entries: &[(&[u8], u64, u8, &[u8])]) -> Vec<u8> {
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
    for (key, version, meta, value) in entries {
        len(&mut out, key.len());
        out.extend_from_slice(key);
        out.extend_from_slice(&version.to_be_bytes());
        out.push(0x01);
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
    drop(s);
    let s = dir.open();
    assert_eq!(
        s.dump().unwrap().len(),
        2,
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
    assert!(
        err.to_string()
            .starts_with("Corrupted value of key 64726f70: unknown drop kind 7"),
        "{err}"
    );
    let mut short = record(Some(&[b"p"]), 1, &[]);
    short.pop();
    leave_record(&dir, short);
    assert!(
        ultragraphdb_store_kv::FjallStore::open(
            dir.path(),
            ultragraphdb_store_kv::Durability::Process
        )
        .is_err()
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
    assert!(err.to_string().starts_with("Key with size 65001"));
    assert_eq!(held(&s, b"\x00a1"), vec![(3, false)]);
    assert!(
        s.drop_all(vec![Entry::deletion(vec![0; 40_000], 1)])
            .is_err()
    );
    assert_eq!(held(&s, b"\x00a1"), vec![(3, false)]);
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
