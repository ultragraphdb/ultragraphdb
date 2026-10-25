/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::Entry;

#[test]
fn test_a_put_carries_its_key_version_meta_and_value() {
    let e = Entry::put(b"k".to_vec(), 7, 0x04, b"v".to_vec());
    assert_eq!(
        (e.key(), e.version(), e.meta(), e.value()),
        (&b"k"[..], 7, 0x04, Some(&b"v"[..]))
    );
    assert!(!e.discard());
    assert!(!e.is_deletion());
    let edge = Entry::put(Vec::new(), u64::MAX, 0xFF, vec![0; 3]);
    assert_eq!(
        (edge.key(), edge.version(), edge.meta(), edge.value()),
        (&b""[..], u64::MAX, 0xFF, Some(&[0u8; 3][..]))
    );
}

#[test]
fn test_a_put_of_an_empty_value_is_not_a_deletion() {
    // Dgraph writes empty values (meta 0x10, BitEmptyPosting); only Entry::deletion hides a key.
    let e = Entry::put(b"k".to_vec(), 2, 0x10, Vec::new());
    assert!(!e.is_deletion());
    assert_eq!(e.value(), Some(&b""[..]));
}

#[test]
fn test_a_deletion_has_no_value_meta_or_discard() {
    let e = Entry::deletion(b"k".to_vec(), 9);
    assert!(e.is_deletion());
    assert_eq!(
        (e.key(), e.value(), e.version(), e.meta(), e.discard()),
        (&b"k"[..], None, 9, 0, false)
    );
}

#[test]
fn test_with_discard_sets_the_flag_and_nothing_else() {
    let put = Entry::put(b"k".to_vec(), 1, 0x08, b"v".to_vec()).with_discard();
    assert!(put.discard());
    assert_eq!(
        (
            put.key(),
            put.version(),
            put.meta(),
            put.value(),
            put.is_deletion()
        ),
        (&b"k"[..], 1, 0x08, Some(&b"v"[..]), false)
    );
    let deletion = Entry::deletion(b"k".to_vec(), 3).with_discard();
    assert!(deletion.discard() && deletion.is_deletion());
    assert!(
        deletion.with_discard().discard(),
        "setting it twice keeps it set"
    );
}
