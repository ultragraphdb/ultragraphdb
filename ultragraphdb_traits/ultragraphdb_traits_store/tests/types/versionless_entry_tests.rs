/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::VersionlessEntry;

#[test]
fn test_a_put_holds_its_key_and_value() {
    let entry = VersionlessEntry::put(b"term".to_vec(), b"uid".to_vec());
    assert_eq!(entry.key(), b"term");
    assert_eq!(entry.value(), Some(&b"uid"[..]));
    assert!(!entry.is_removal());
}

#[test]
fn test_a_put_of_an_empty_value_is_not_a_removal() {
    // An empty value is a value: only VersionlessEntry::removal removes a key, as Entry::deletion alone hides one.
    let entry = VersionlessEntry::put(Vec::new(), Vec::new());
    assert_eq!(entry.key(), b"");
    assert_eq!(entry.value(), Some(&b""[..]));
    assert!(!entry.is_removal());
}

#[test]
fn test_a_removal_holds_its_key_and_no_value() {
    let entry = VersionlessEntry::removal(b"term".to_vec());
    assert_eq!(entry.key(), b"term");
    assert_eq!(entry.value(), None);
    assert!(entry.is_removal());
    assert_ne!(entry, VersionlessEntry::put(b"term".to_vec(), Vec::new()));
}
