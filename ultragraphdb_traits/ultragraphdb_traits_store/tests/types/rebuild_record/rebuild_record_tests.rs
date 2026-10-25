/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::RebuildRecord;

#[test]
fn test_a_record_names_its_rebuild_and_prefixes_in_order() {
    let record = RebuildRecord::new(12, vec![b"\x04a".to_vec(), b"\x00a".to_vec()]);
    assert_eq!(record.id(), 12);
    assert_eq!(record.prefixes(), [b"\x04a".to_vec(), b"\x00a".to_vec()]);
    assert_eq!(
        record.into_prefixes(),
        vec![b"\x04a".to_vec(), b"\x00a".to_vec()]
    );
    assert_eq!(RebuildRecord::new(u64::MAX, Vec::new()).id(), u64::MAX);
}

#[test]
fn test_a_record_covers_the_keys_under_any_of_its_prefixes() {
    let record = RebuildRecord::new(3, vec![b"ab".to_vec(), b"\x02x".to_vec()]);
    for key in [&b"ab"[..], b"abc", b"ab\x00", b"\x02x", b"\x02xyz"] {
        assert!(record.covers(key), "{key:?}");
    }
    for key in [&b""[..], b"a", b"ac", b"b", b"\x02", b"\x02y", b"xab"] {
        assert!(!record.covers(key), "{key:?}");
    }
}

#[test]
fn test_an_empty_prefix_covers_every_key_and_no_prefix_covers_none() {
    let everything = RebuildRecord::new(4, vec![Vec::new()]);
    assert!(everything.covers(b"") && everything.covers(&[0xFF; 4]));
    let nothing = RebuildRecord::new(4, Vec::new());
    assert!(!nothing.covers(b"") && !nothing.covers(b"a"));
}
