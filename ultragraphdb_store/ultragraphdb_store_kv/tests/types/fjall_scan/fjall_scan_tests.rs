/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot};

use crate::test_store::{TestDir, put, scanned, write};

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
    assert_eq!(scanned(&s, &Scan::range(b"b".to_vec(), None), 1).len(), 3);
    assert_eq!(
        scanned(&s, &Scan::prefix(vec![0xFF]), 1),
        vec![(vec![0xFF, 0xFF], 1)]
    );
    assert!(scanned(&s, &Scan::range(b"c".to_vec(), Some(b"b".to_vec())), 1).is_empty());
    assert!(
        scanned(
            &s,
            &Scan::prefix(b"b".to_vec()).reverse().seek(b"a".to_vec()),
            1
        )
        .is_empty()
    );
    assert_eq!(
        scanned(
            &s,
            &Scan::range(b"a".to_vec(), Some(b"c".to_vec()))
                .reverse()
                .seek(b"z".to_vec()),
            1
        )
        .len(),
        2
    );
    let snapshot = s.snapshot(0).unwrap();
    assert_eq!(
        snapshot.scan(&Scan::prefix(Vec::new())).count(),
        0,
        "nothing at or below 0"
    );
}

#[test]
fn test_a_reverse_scan_stops_each_key_at_its_newest_deletion() {
    let dir = TestDir::new("scan-del");
    let s = dir.open();
    write(
        &s,
        vec![
            put(b"a", 1, b""),
            ultragraphdb_traits_store::Entry::deletion(b"a".to_vec(), 2),
            put(b"a", 3, b""),
        ],
    );
    write(&s, vec![put(b"b", 1, b"")]);
    let all = |ts| scanned(&s, &Scan::prefix(Vec::new()).reverse().all_versions(), ts);
    assert_eq!(all(5), vec![(b"b".to_vec(), 1), (b"a".to_vec(), 3)]);
    assert_eq!(all(2), vec![(b"b".to_vec(), 1)]);
    assert_eq!(all(1), vec![(b"b".to_vec(), 1), (b"a".to_vec(), 1)]);
}
