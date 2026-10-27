/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::DATA_KEYSPACE;
use ultragraphdb_traits_store::{Entry, Item, Store, StoreSnapshot};

use ultragraphdb_tool_test::{TestDir, put, write, write_raw, write_raw_in};

#[test]
fn test_a_snapshot_reads_at_its_timestamp_and_releases_the_discard_timestamp_when_dropped() {
    let dir = TestDir::new("snapshot");
    let s = dir.open();
    write(&s, vec![put(b"a", 2, b"x"), put(b"b", 4, b"y")]);
    let snapshot = s.snapshot(3).unwrap();
    assert_eq!(snapshot.read_ts(), 3);
    let got = snapshot.get_many(&[b"a".as_slice(), b"b", b"c"]).unwrap();
    assert_eq!(
        got,
        vec![
            Some(Item::new(b"a".to_vec(), 2, 0x04, false, b"x".to_vec())),
            None,
            None
        ]
    );
    assert_eq!(
        snapshot.get_many::<&[u8]>(&[]).unwrap(),
        Vec::<Option<Item>>::new()
    );
    assert_eq!(
        format!("{snapshot:?}"),
        "FjallSnapshot { read_ts: 3, banned: {}, .. }"
    );
    s.set_discard_ts(5).unwrap();
    assert_eq!(s.discard_ts(), 3);
    let twin = s.snapshot(3).unwrap();
    drop(snapshot);
    assert_eq!(s.discard_ts(), 3, "the second reader at 3 still holds it");
    drop(twin);
    assert_eq!(s.discard_ts(), 5);
}

#[test]
fn test_a_read_returns_the_versions_meta_byte_and_discard_flag() {
    let dir = TestDir::new("snapshot-flags");
    let s = dir.open();
    write(
        &s,
        vec![
            Entry::put(b"k".to_vec(), 5, 0x08, b"complete".to_vec()).with_discard(),
            Entry::put(b"k".to_vec(), 7, 0x04, b"delta".to_vec()),
            Entry::put(b"e".to_vec(), 1, 0x10, Vec::new()),
        ],
    );
    let snapshot = s.snapshot(u64::MAX).unwrap();
    assert_eq!(
        snapshot.get(b"k").unwrap(),
        Some(Item::new(b"k".to_vec(), 7, 0x04, false, b"delta".to_vec()))
    );
    assert_eq!(
        s.get_at(b"k", 6).unwrap(),
        Some(Item::new(
            b"k".to_vec(),
            5,
            0x08,
            true,
            b"complete".to_vec()
        ))
    );
    assert_eq!(
        snapshot.get(b"e").unwrap(),
        Some(Item::new(b"e".to_vec(), 1, 0x10, false, Vec::new())),
        "an empty value is a value, not a deletion"
    );
}

#[test]
fn test_an_undecodable_version_is_refused_as_corrupted() {
    let dir = TestDir::new("snapshot-corrupt-value");
    drop(dir.open());
    write_raw(&dir, b"k", 3, &[0x04, 0x80, b'v']);
    write_raw(&dir, b"j", 3, &[0x04]);
    let s = dir.open();
    assert_eq!(
        s.get_at(b"k", 3),
        Err(StoreError::Corrupted(
            b"k".to_vec(),
            "unknown version flags 0x80".to_string()
        ))
    );
    assert_eq!(
        s.get_at(b"j", 3).unwrap_err().to_string(),
        "Corrupted value of key 6a: value too short to hold the version prefix"
    );
    assert_eq!(s.get_at(b"k", 2).unwrap(), None, "no version at or below 2");
}

#[test]
fn test_a_stored_key_cut_short_after_its_terminator_is_refused_not_a_panic() {
    // A stored key holding the key and its terminator but fewer than the version's eight bytes, as a torn or foreign
    // write could leave it: it sorts among the key's versions and must be reported, not sliced.
    let dir = TestDir::new("snapshot-short-key");
    drop(dir.open());
    write_raw_in(
        &dir,
        DATA_KEYSPACE,
        &[b'k', 0x00, 0x01, 0x05],
        &[0x04, 0x00],
    );
    write_raw_in(&dir, DATA_KEYSPACE, &[0x00, 0x01, 0xFF], &[0x04, 0x00]);
    // One byte too long: its last eight bytes would read as a version the key was never written at.
    let mut long = vec![b'j', 0x00, 0x01];
    long.extend_from_slice(&[0x00; 9]);
    write_raw_in(&dir, DATA_KEYSPACE, &long, &[0x04, 0x00]);
    let s = dir.open();
    assert_eq!(
        s.get_at(b"j", u64::MAX),
        Err(StoreError::Corrupted(
            long,
            "stored key of 12 bytes does not end in a terminated version".to_string()
        ))
    );
    assert_eq!(
        s.get_at(b"k", u64::MAX),
        Err(StoreError::Corrupted(
            vec![b'k', 0x00, 0x01, 0x05],
            "stored key of 4 bytes does not end in a terminated version".to_string()
        ))
    );
    assert_eq!(
        s.get_at(b"", u64::MAX),
        Err(StoreError::Corrupted(
            vec![0x00, 0x01, 0xFF],
            "stored key of 3 bytes does not end in a terminated version".to_string()
        ))
    );
}
