/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::Item;

/// An item's parts: key, version, meta, discard and value.
type Parts = (&'static [u8], u64, u8, bool, &'static [u8]);

#[test]
fn test_an_item_returns_what_it_was_built_from() {
    let cases: [Parts; 3] = [
        (b"k", 5, 0x08, true, b"v"),
        (b"", 0, 0x00, false, b""),
        (&[0xFF, 0x00], u64::MAX, 0xFF, false, &[0x00; 4]),
    ];
    for (key, version, meta, discard, value) in cases {
        let item = Item::new(key.to_vec(), version, meta, discard, value.to_vec());
        assert_eq!(
            (
                item.key(),
                item.version(),
                item.meta(),
                item.discard(),
                item.value()
            ),
            (key, version, meta, discard, value)
        );
        assert_eq!(item.into_value(), value.to_vec());
    }
}
