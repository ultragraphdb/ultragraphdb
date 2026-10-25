/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Rebuild records in the metadata keyspace: synced when written, removed by a batch with its entries, cleared by a
//! drop of everything.

use ultragraphdb_traits_store::{PersistMode, RebuildRecord, Store, WriteBatch};

use crate::test_store::{TestDir, ns_key, put};

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
