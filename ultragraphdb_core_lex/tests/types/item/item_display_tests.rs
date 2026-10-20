/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_lex::{ItemType, Lexer};
use ultragraphdb_test_bdd::{load_golden, testdata_dir};

#[derive(Deserialize)]
struct ItemStringCase {
    typ: i32,
    val: String,
    string: String,
}

#[test]
fn test_item_string_golden() {
    let golden = load_golden::<ItemStringCase>(
        &testdata_dir("ultragraphdb_core_lex").join("golden/lex_item_string.json"),
    );
    assert!(!golden.cases().is_empty());
    for case in golden.cases() {
        let mut l = Lexer::new(&case.val);
        while l.next().is_some() {}
        l.emit(ItemType::new(case.typ));
        let mut it = l.new_iterator();
        assert!(it.next());
        assert_eq!(
            it.item().to_string(),
            case.string,
            "typ {} val {:?}",
            case.typ,
            case.val
        );
    }
}

#[test]
fn test_eof_item_string() {
    let mut l = Lexer::new("ignored");
    l.emit(ultragraphdb_core_lex::ITEM_EOF);
    let mut it = l.new_iterator();
    assert!(it.next());
    assert_eq!(it.item().to_string(), "EOF");
}

#[test]
fn test_out_of_range_item_string() {
    let l = Lexer::new("");
    let it = l.new_iterator();
    // The out-of-range item has type 0, which is EOF.
    assert_eq!(it.item().to_string(), "EOF");
}
