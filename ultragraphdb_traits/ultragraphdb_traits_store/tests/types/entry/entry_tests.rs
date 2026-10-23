/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::Entry;

#[test]
fn test_a_put_carries_its_version_meta_and_value() {
    let e = Entry::put(b"k".to_vec(), 7, 0x04, b"v".to_vec());
    assert_eq!(
        (e.key(), e.version(), e.meta(), e.value()),
        (&b"k"[..], 7, 0x04, Some(&b"v"[..]))
    );
    assert!(!e.discard());
    assert!(!e.is_deletion());
}

#[test]
fn test_a_deletion_has_no_value() {
    let e = Entry::deletion(b"k".to_vec(), 9);
    assert!(e.is_deletion());
    assert_eq!((e.value(), e.version(), e.meta()), (None, 9, 0));
}

#[test]
fn test_with_discard_sets_the_flag() {
    assert!(
        Entry::put(b"k".to_vec(), 1, 0x08, vec![])
            .with_discard()
            .discard()
    );
}
