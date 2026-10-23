/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{Store, StoreSnapshot};

use crate::test_store::{TestDir, put, write};

#[test]
fn test_a_snapshot_reads_at_its_timestamp_and_releases_the_discard_timestamp_when_dropped() {
    let dir = TestDir::new("snapshot");
    let s = dir.open();
    write(&s, vec![put(b"a", 2, b"x"), put(b"b", 4, b"y")]);
    let snapshot = s.snapshot(3).unwrap();
    assert_eq!(snapshot.read_ts(), 3);
    let got = snapshot.get_many(&[b"a".as_slice(), b"b", b"c"]).unwrap();
    assert_eq!(
        got.iter().map(Option::is_some).collect::<Vec<_>>(),
        vec![true, false, false]
    );
    assert!(format!("{snapshot:?}").contains("read_ts: 3"));
    s.set_discard_ts(5).unwrap();
    assert_eq!(s.discard_ts(), 3);
    let twin = s.snapshot(3).unwrap();
    drop(snapshot);
    assert_eq!(s.discard_ts(), 3, "the second reader at 3 still holds it");
    drop(twin);
    assert_eq!(s.discard_ts(), 5);
}
