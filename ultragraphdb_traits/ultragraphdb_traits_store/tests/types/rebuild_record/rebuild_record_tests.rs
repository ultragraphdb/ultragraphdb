/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::RebuildRecord;

#[test]
fn test_a_record_names_its_rebuild_and_prefixes() {
    let record = RebuildRecord::new(12, vec![b"\x00a".to_vec(), b"\x04a".to_vec()]);
    assert_eq!(record.id(), 12);
    assert_eq!(record.prefixes(), [b"\x00a".to_vec(), b"\x04a".to_vec()]);
    assert_eq!(record.clone(), record);
    assert_eq!(record.into_prefixes().len(), 2);
}

#[test]
fn test_a_record_covers_the_keys_under_its_prefixes() {
    let record = RebuildRecord::new(3, vec![b"ab".to_vec()]);
    assert!(record.covers(b"abc"));
    assert!(!record.covers(b"ac"));
    assert!(!RebuildRecord::new(4, Vec::new()).covers(b""));
}
