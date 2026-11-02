/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Rebuild records in the metadata keyspace: synced when written, removed by a batch with its entries, cleared by a
//! drop of everything.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::META_KEYSPACE;
use ultragraphdb_traits_store::{PersistMode, RebuildRecord, Store, WriteBatch};

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_test::{TestDir, ns_key, put, write_raw_in};

fn record(id: u64) -> RebuildRecord {
    RebuildRecord::new(id, vec![ns_key(0, b"idx"), vec![0x04, 1, 2]])
}

#[test]
fn test_a_record_is_held_until_a_batch_removes_it() -> TestResult {
    let dir = TestDir::new("records");
    let s = dir.open()?;
    s.write_rebuild_record(&record(4))?;
    s.write_rebuild_record(&record(300))?;
    assert_eq!(s.rebuild_records()?, [record(4), record(300)]);
    let mut batch = WriteBatch::new();
    batch.push(put(&ns_key(0, b"k"), 5, b"swapped"));
    batch.remove_rebuild_record(4);
    s.apply(batch, PersistMode::Buffer)?;
    assert_eq!(s.rebuild_records()?, [record(300)]);
    let replaced = RebuildRecord::new(300, Vec::new());
    s.write_rebuild_record(&replaced)?;
    assert_eq!(s.rebuild_records()?, [replaced]);
    Ok(())
}

#[test]
fn test_records_survive_a_reopen() -> TestResult {
    let dir = TestDir::new("records-reopen");
    let s = dir.open()?;
    s.write_rebuild_record(&record(4))?;
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(reopened.rebuild_records()?, [record(4)]);
    Ok(())
}

#[test]
fn test_dropping_everything_removes_every_record() -> TestResult {
    let dir = TestDir::new("records-drop-all");
    let s = dir.open()?;
    s.write_rebuild_record(&record(4))?;
    s.drop_all(Vec::new())?;
    assert!(s.rebuild_records()?.is_empty());
    Ok(())
}

#[test]
fn test_records_are_listed_by_number_whatever_their_prefixes() -> TestResult {
    // 256 sorts after 1 only in big-endian order.
    let dir = TestDir::new("records-order");
    let s = dir.open()?;
    let records = [
        RebuildRecord::new(u64::MAX, vec![Vec::new(), vec![0x00]]),
        RebuildRecord::new(256, vec![vec![0xFF; 3]]),
        RebuildRecord::new(1, Vec::new()),
        RebuildRecord::new(0, vec![ns_key(0, b"")]),
    ];
    for record in &records {
        s.write_rebuild_record(record)?;
    }
    let expected = [
        records[3].clone(),
        records[2].clone(),
        records[1].clone(),
        records[0].clone(),
    ];
    assert_eq!(s.rebuild_records()?, expected);
    let mut batch = WriteBatch::new();
    batch.remove_rebuild_record(77);
    s.apply(batch, PersistMode::Buffer)?;
    assert_eq!(
        s.rebuild_records()?,
        expected,
        "a record the store does not hold is left alone"
    );
    drop(s);
    assert_eq!(dir.open()?.rebuild_records()?, expected);
    Ok(())
}

#[test]
fn test_an_unreadable_record_is_refused_as_corrupted() -> TestResult {
    let record_key = |id: &[u8]| [b"rebuild/".as_slice(), id].concat();
    let eight = record_key(&5u64.to_be_bytes());
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
            record_key(&[0, 5]),
            vec![0, 0, 0, 0],
            "a rebuild record's number is eight bytes",
        ),
    ] {
        let dir = TestDir::new("records-corrupt");
        drop(dir.open()?);
        write_raw_in(&dir, META_KEYSPACE, &key, &value)?;
        assert_eq!(
            dir.open()?.rebuild_records(),
            Err(StoreError::Corrupted(key.clone(), reason.to_string())),
            "{reason}"
        );
    }
    Ok(())
}
