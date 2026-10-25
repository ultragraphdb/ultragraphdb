/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Rebuild records in the metadata keyspace: synced when written, removed by a batch with its entries, cleared by a
//! drop of everything.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::META_KEYSPACE;
use ultragraphdb_traits_store::{PersistMode, RebuildRecord, Store, WriteBatch};

use crate::test_store::{TestDir, ns_key, put, write_raw_in};

fn record(id: u64) -> RebuildRecord {
    RebuildRecord::new(id, vec![ns_key(0, b"idx"), vec![0x04, 1, 2]])
}

#[test]
fn test_a_record_is_held_until_a_batch_removes_it() {
    let dir = TestDir::new("records");
    let s = dir.open();
    s.write_rebuild_record(&record(4)).unwrap();
    s.write_rebuild_record(&record(300)).unwrap();
    assert_eq!(s.rebuild_records().unwrap(), [record(4), record(300)]);
    let mut batch = WriteBatch::new();
    batch.push(put(&ns_key(0, b"k"), 5, b"swapped"));
    batch.remove_rebuild_record(4);
    s.apply(batch, PersistMode::Buffer).unwrap();
    assert_eq!(s.rebuild_records().unwrap(), [record(300)]);
    let replaced = RebuildRecord::new(300, Vec::new());
    s.write_rebuild_record(&replaced).unwrap();
    assert_eq!(s.rebuild_records().unwrap(), [replaced]);
}

#[test]
fn test_records_survive_a_reopen() {
    let dir = TestDir::new("records-reopen");
    let s = dir.open();
    s.write_rebuild_record(&record(4)).unwrap();
    drop(s);
    let s = dir.open();
    assert_eq!(s.rebuild_records().unwrap(), [record(4)]);
}

#[test]
fn test_dropping_everything_removes_every_record() {
    let dir = TestDir::new("records-drop-all");
    let s = dir.open();
    s.write_rebuild_record(&record(4)).unwrap();
    s.drop_all(Vec::new()).unwrap();
    assert!(s.rebuild_records().unwrap().is_empty());
}

#[test]
fn test_records_are_listed_by_number_whatever_their_prefixes() {
    // 256 sorts after 1 only in big-endian order.
    let dir = TestDir::new("records-order");
    let s = dir.open();
    let records = [
        RebuildRecord::new(u64::MAX, vec![Vec::new(), vec![0x00]]),
        RebuildRecord::new(256, vec![vec![0xFF; 3]]),
        RebuildRecord::new(1, Vec::new()),
        RebuildRecord::new(0, vec![ns_key(0, b"")]),
    ];
    records
        .iter()
        .for_each(|r| s.write_rebuild_record(r).unwrap());
    let expected = [
        records[3].clone(),
        records[2].clone(),
        records[1].clone(),
        records[0].clone(),
    ];
    assert_eq!(s.rebuild_records().unwrap(), expected);
    let mut batch = WriteBatch::new();
    batch.remove_rebuild_record(77);
    s.apply(batch, PersistMode::Buffer).unwrap();
    assert_eq!(
        s.rebuild_records().unwrap(),
        expected,
        "a record the store does not hold is left alone"
    );
    drop(s);
    assert_eq!(dir.open().rebuild_records().unwrap(), expected);
}

#[test]
fn test_an_unreadable_record_is_refused_as_corrupted() {
    let key = |id: &[u8]| [b"rebuild/".as_slice(), id].concat();
    let eight = key(&5u64.to_be_bytes());
    for (key, value, reason) in [
        (
            eight.clone(),
            vec![0, 0],
            "a rebuild record ends after 2 of 4 bytes",
        ),
        (
            eight.clone(),
            vec![0, 0, 0, 1, 0, 0, 0, 3, b'a'],
            "a rebuild record ends after 1 of 3 bytes",
        ),
        (
            eight.clone(),
            vec![0, 0, 0, 0, 0xAA],
            "1 bytes after the rebuild record",
        ),
        (
            key(&[0, 5]),
            vec![0, 0, 0, 0],
            "a rebuild record's number is eight bytes",
        ),
    ] {
        let dir = TestDir::new("records-corrupt");
        drop(dir.open());
        write_raw_in(&dir, META_KEYSPACE, &key, &value);
        assert_eq!(
            dir.open().rebuild_records(),
            Err(StoreError::Corrupted(key.clone(), reason.to_string())),
            "{reason}"
        );
    }
}
