/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::DATA_KEYSPACE;
use ultragraphdb_traits_store::{Entry, Item, Scan, Store, StoreSnapshot};

use crate::test_store::{TestDir, put, scanned, stored, write, write_raw, write_raw_in};

fn keys(got: &[(Vec<u8>, u64)]) -> Vec<&[u8]> {
    got.iter().map(|(k, _)| k.as_slice()).collect()
}

#[test]
fn test_ranges_seeks_and_empty_scans() {
    let dir = TestDir::new("scan");
    let s = dir.open();
    write(
        &s,
        vec![
            put(b"a", 1, b""),
            put(b"b", 1, b""),
            put(b"c", 1, b""),
            put(&[0xFF, 0xFF], 1, b""),
        ],
    );
    let from_b = scanned(&s, &Scan::range(b"b".to_vec(), None), 1);
    assert_eq!(keys(&from_b), vec![&b"b"[..], b"c", &[0xFF, 0xFF]]);
    assert_eq!(
        scanned(&s, &Scan::prefix(vec![0xFF]), 1),
        vec![(vec![0xFF, 0xFF], 1)]
    );
    assert!(scanned(&s, &Scan::range(b"c".to_vec(), Some(b"b".to_vec())), 1).is_empty());
    assert!(
        scanned(&s, &Scan::range(b"b".to_vec(), Some(b"b".to_vec())), 1).is_empty(),
        "an empty range"
    );
    assert!(
        scanned(
            &s,
            &Scan::prefix(b"b".to_vec()).reverse().seek(b"a".to_vec()),
            1
        )
        .is_empty()
    );
    let reversed = scanned(
        &s,
        &Scan::range(b"a".to_vec(), Some(b"c".to_vec()))
            .reverse()
            .seek(b"z".to_vec()),
        1,
    );
    assert_eq!(keys(&reversed), vec![&b"b"[..], b"a"]);
    let snapshot = s.snapshot(0).unwrap();
    assert_eq!(
        snapshot.scan(&Scan::prefix(Vec::new())).count(),
        0,
        "nothing at or below 0"
    );
}

#[test]
fn test_a_seek_outside_the_range_is_held_to_it() {
    let dir = TestDir::new("scan-seek-bounds");
    let s = dir.open();
    write(
        &s,
        vec![
            put(b"a", 1, b""),
            put(b"b", 1, b""),
            put(b"b\x00", 1, b""),
            put(b"ba", 1, b""),
            put(b"c", 1, b""),
        ],
    );
    let before = scanned(
        &s,
        &Scan::range(b"b".to_vec(), Some(b"c".to_vec())).seek(b"a".to_vec()),
        1,
    );
    assert_eq!(
        keys(&before),
        vec![&b"b"[..], b"b\x00", b"ba"],
        "a forward seek before the start begins at the start"
    );
    let reverse = scanned(
        &s,
        &Scan::prefix(Vec::new()).reverse().seek(b"b".to_vec()),
        1,
    );
    assert_eq!(
        keys(&reverse),
        vec![&b"b"[..], b"a"],
        "a reverse seek at b leaves out the longer keys beginning with it"
    );
    let forward = scanned(&s, &Scan::prefix(Vec::new()).seek(b"b\x00".to_vec()), 1);
    assert_eq!(keys(&forward), vec![&b"b\x00"[..], b"ba", b"c"]);
}

#[test]
fn test_a_reverse_scan_stops_each_key_at_its_newest_deletion() {
    let dir = TestDir::new("scan-del");
    let s = dir.open();
    write(
        &s,
        vec![
            put(b"a", 1, b""),
            Entry::deletion(b"a".to_vec(), 2),
            put(b"a", 3, b""),
        ],
    );
    write(&s, vec![put(b"b", 1, b"")]);
    let all = |ts| scanned(&s, &Scan::prefix(Vec::new()).reverse().all_versions(), ts);
    assert_eq!(all(5), vec![(b"b".to_vec(), 1), (b"a".to_vec(), 3)]);
    assert_eq!(all(2), vec![(b"b".to_vec(), 1)]);
    assert_eq!(all(1), vec![(b"b".to_vec(), 1), (b"a".to_vec(), 1)]);
    let forward = |ts| scanned(&s, &Scan::prefix(Vec::new()).all_versions(), ts);
    assert_eq!(forward(5), vec![(b"a".to_vec(), 3), (b"b".to_vec(), 1)]);
    assert_eq!(forward(2), vec![(b"b".to_vec(), 1)]);
}

#[test]
fn test_a_failed_read_is_the_last_item_in_either_direction() {
    let dir = TestDir::new("scan-corrupt");
    let s = dir.open();
    write(
        &s,
        vec![put(b"a", 1, b""), put(b"c", 1, b""), put(b"d", 1, b"")],
    );
    drop(s);
    write_raw(&dir, b"b", 1, &[0x04, 0x80]);
    let s = dir.open();
    let corrupted = StoreError::Corrupted(b"b".to_vec(), "unknown version flags 0x80".to_string());
    let snapshot = s.snapshot(1).unwrap();
    let item = |k: &[u8]| Ok(Item::new(k.to_vec(), 1, 0x04, false, Vec::new()));
    let forward: Vec<_> = snapshot.scan(&Scan::prefix(Vec::new())).collect();
    assert_eq!(forward, vec![item(b"a"), Err(corrupted.clone())]);
    let reverse: Vec<_> = snapshot.scan(&Scan::prefix(Vec::new()).reverse()).collect();
    assert_eq!(reverse, vec![item(b"d"), item(b"c"), Err(corrupted)]);
}

#[test]
fn test_an_undecodable_stored_key_is_refused_as_corrupted() {
    // A zero byte of the user key not followed by the escape byte.
    let dir = TestDir::new("scan-unescaped");
    drop(dir.open());
    let mut raw = b"x\x00y".to_vec();
    raw.extend_from_slice(&stored(b"", 1));
    write_raw_in(&dir, DATA_KEYSPACE, &raw, &[0x04, 0x00]);
    let s = dir.open();
    let got: Vec<_> = s
        .snapshot(1)
        .unwrap()
        .scan(&Scan::prefix(Vec::new()))
        .collect();
    assert_eq!(
        got,
        vec![Err(StoreError::Corrupted(
            raw.clone(),
            "stored key holds an unescaped zero byte".to_string()
        ))]
    );
    assert_eq!(
        s.dump(),
        Err(StoreError::Corrupted(
            raw,
            "stored key holds an unescaped zero byte".to_string()
        ))
    );
}
