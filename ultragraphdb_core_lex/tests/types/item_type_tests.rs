/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{ITEM_EOF, ITEM_ERROR, ItemType};

// A client crate declares its item types as constants, from 5 up, as Go's per-package `iota` blocks do.
const ITEM_TEXT: ItemType = ItemType::new(5);
const ITEM_NAME: ItemType = ItemType::new(9);

#[test]
fn test_reserved_item_types() {
    assert_eq!(ITEM_EOF.value(), 0);
    assert_eq!(ITEM_ERROR.value(), 1);
    assert_ne!(ITEM_EOF, ITEM_ERROR);
}

#[test]
fn test_client_constants_match() {
    let typ = ItemType::new(9);
    let what = match typ {
        ITEM_EOF => "eof",
        ITEM_TEXT => "text",
        ITEM_NAME => "name",
        _ => "other",
    };
    assert_eq!(what, "name");
}

#[test]
fn test_display_is_the_number() {
    assert_eq!(ITEM_TEXT.to_string(), "5");
    assert_eq!(ItemType::new(-3).to_string(), "-3");
}
