/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::Item;

#[test]
fn test_an_item_returns_what_it_was_built_from() {
    let item = Item::new(b"k".to_vec(), 5, 0x08, true, b"v".to_vec());
    assert_eq!(
        (
            item.key(),
            item.version(),
            item.meta(),
            item.discard(),
            item.value()
        ),
        (&b"k"[..], 5, 0x08, true, &b"v"[..])
    );
    assert_eq!(item.clone().into_value(), b"v".to_vec());
}
