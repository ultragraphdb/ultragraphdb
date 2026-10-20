/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{ITEM_ERROR, ItemType, LexErrorEnum, Lexer};

const ITEM_WORD: ItemType = ItemType::new(5);

#[test]
fn test_item_getters() {
    let mut l = Lexer::new("ab\ncd");
    l.next();
    l.next();
    l.emit(ITEM_WORD);
    l.next();
    l.ignore();
    l.next();
    l.next();
    l.emit(ITEM_WORD);
    let mut it = l.new_iterator();
    assert!(it.next());
    let first = it.item();
    assert_eq!(first.typ(), ITEM_WORD);
    assert_eq!(first.val(), "ab");
    assert_eq!((first.line(), first.column()), (1, 0));
    assert!(it.next());
    let second = it.item();
    assert_eq!(second.val(), "cd");
    assert_eq!((second.line(), second.column()), (2, 0));
}

#[test]
fn test_item_errorf() {
    let mut l = Lexer::new("  x");
    l.next();
    l.next();
    l.ignore();
    l.next();
    l.emit(ITEM_WORD);
    let mut it = l.new_iterator();
    assert!(it.next());
    let err = it
        .item()
        .errorf(format_args!("Expected {} but got {}", "y", it.item().val()));
    assert_eq!(err.to_string(), "line 1 column 2: Expected y but got x");
    assert_eq!(
        err.kind(),
        &LexErrorEnum::Located {
            line: 1,
            column: 2,
            message: "Expected y but got x".to_string()
        }
    );
}

#[test]
fn test_error_item_owns_its_text() {
    let mut l = Lexer::new("q");
    assert!(l.errorf("bad").is_none());
    let mut it = l.new_iterator();
    assert!(it.next());
    assert_eq!(it.item().typ(), ITEM_ERROR);
    assert_eq!(it.item().val(), "while lexing q at line 1 column 0: bad");
}

#[test]
fn test_item_outlives_iterator() {
    let mut l = Lexer::new("abc");
    l.next();
    l.emit(ITEM_WORD);
    let item = {
        let mut it = l.new_iterator();
        it.next();
        it.item().clone()
    };
    assert_eq!(item.val(), "a");
}
