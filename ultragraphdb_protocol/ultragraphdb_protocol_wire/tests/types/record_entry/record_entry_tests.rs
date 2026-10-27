/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::RecordEntry;

#[test]
fn test_an_entry_holds_its_key_and_value_or_removal() {
    let put = RecordEntry::new(b"k", Some(b"v"));
    assert_eq!(put.key(), b"k");
    assert_eq!(put.value(), Some(&b"v"[..]));
    let removal = RecordEntry::new(b"k", None);
    assert_eq!(removal.value(), None);
    assert_ne!(put, removal);
    assert_eq!(RecordEntry::new(b"", Some(b"")).value(), Some(&b""[..]));
}
