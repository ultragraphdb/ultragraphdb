/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store's layout in fjall and its durability modes.

use std::time::Duration;

use fjall::{Database, KeyspaceCreateOptions, Readable};
use ultragraphdb_store_kv::{DATA_KEYSPACE, Durability, FjallStore, META_KEYSPACE};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use ultragraphdb_tool_test::stored;
use ultragraphdb_tool_test::{TestDir, put, write};

#[test]
fn test_versions_are_part_of_the_stored_key_and_the_metadata_a_value_prefix() {
    let dir = TestDir::new("layout");
    let s = dir.open();
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"a\x00b".to_vec(), 3, 0x08, b"v".to_vec()).with_discard());
    batch.push(Entry::deletion(b"a\x00b".to_vec(), 5));
    batch.set_applied_ts(5);
    s.apply(batch, PersistMode::Sync).unwrap();
    drop(s);
    let db = Database::builder(dir.path()).open().unwrap();
    let data = db
        .keyspace(DATA_KEYSPACE, KeyspaceCreateOptions::default)
        .unwrap();
    let meta = db
        .keyspace(META_KEYSPACE, KeyspaceCreateOptions::default)
        .unwrap();
    let snapshot = db.snapshot();
    let raw: Vec<(Vec<u8>, Vec<u8>)> = snapshot
        .iter(&data)
        .map(|g| g.into_inner().unwrap())
        .map(|(k, v)| (k.to_vec(), v.to_vec()))
        .collect();
    let mut five = b"a\x00\xffb\x00\x01".to_vec();
    five.extend_from_slice(&(!5u64).to_be_bytes());
    assert_eq!(
        raw,
        vec![
            (five, vec![0x00, 0x02]),
            (stored(b"a\x00b", 3), vec![0x08, 0x01, b'v'])
        ]
    );
    assert_eq!(
        snapshot
            .get(&meta, b"applied_ts")
            .unwrap()
            .unwrap()
            .to_vec(),
        5u64.to_be_bytes().to_vec()
    );
}

#[test]
fn test_the_dump_lists_every_version_with_its_flags() {
    let dir = TestDir::new("dump");
    let s = dir.open();
    write(
        &s,
        vec![
            put(b"b", 1, b"x"),
            Entry::put(b"a".to_vec(), 2, 0x08, b"y".to_vec()).with_discard(),
        ],
    );
    write(&s, vec![Entry::deletion(b"b".to_vec(), 4)]);
    assert_eq!(
        s.dump().unwrap(),
        vec![
            Entry::put(b"a".to_vec(), 2, 0x08, b"y".to_vec()).with_discard(),
            Entry::deletion(b"b".to_vec(), 4),
            put(b"b", 1, b"x"),
        ]
    );
    assert_eq!(
        format!("{s:?}"),
        "FjallStore { durability: Process, applied_ts: 0, poisoned: false, view: View { banned: {}, readers: {}, \
         discard_ts: 0, horizon: None }, .. }"
    );
}

#[test]
fn test_the_filesystem_mode_syncs_every_batch_and_the_process_mode_on_its_period() {
    for (durability, period) in [
        (Durability::Filesystem, Duration::from_secs(60)),
        (Durability::Process, Duration::from_millis(5)),
    ] {
        let dir = TestDir::new("durability");
        let s = FjallStore::open_with_sync_period(dir.path(), durability, period).unwrap();
        write(&s, vec![put(b"k", 1, b"")]);
        std::thread::sleep(Duration::from_millis(50));
        assert!(!s.is_poisoned());
        drop(s);
        assert_eq!(
            dir.open().get_at(b"k", 1).unwrap().map(|i| i.version()),
            Some(1),
            "{durability:?}"
        );
    }
}

#[test]
fn test_the_write_pressure_fjall_stalls_on_is_read_from_the_data_keyspace() {
    let dir = TestDir::new("pressure");
    let s = dir.open();
    assert_eq!((s.l0_table_count(), s.sealed_memtable_count()), (0, 0));
    write(&s, vec![put(b"a", 3, b"v")]);
    s.compact().unwrap();
    // A compaction flushes the memtable and moves every table out of level 0.
    assert_eq!((s.l0_table_count(), s.sealed_memtable_count()), (0, 0));
    assert!(s.data_path().starts_with(dir.path()));
}
