/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{ITEM_EOF, ItemType, LexErrorEnum, Lexer};

const ITEM_WORD: ItemType = ItemType::new(5);

// Lexes "a b c" into three one-rune items and EOF.
fn three_items() -> Lexer<'static> {
    let mut l = Lexer::new("a b c");
    for _ in 0..3 {
        l.next();
        l.emit(ITEM_WORD);
        l.next();
        l.ignore();
    }
    l.emit(ITEM_EOF);
    l
}

#[test]
fn test_next_and_item() {
    let l = three_items();
    let mut it = l.new_iterator();
    let mut vals = Vec::new();
    while it.next() {
        vals.push(it.item().val());
    }
    assert_eq!(vals, vec!["a", "b", "c", ""]);
    // Next keeps advancing past the end and keeps answering false.
    assert!(!it.next());
    assert_eq!(it.save(), 5);
}

#[test]
fn test_item_out_of_range() {
    let l = three_items();
    let mut it = l.new_iterator();
    let before = it.item();
    assert_eq!(
        (before.typ(), before.val(), before.line(), before.column()),
        (ITEM_EOF, "", -1, -1)
    );
    while it.next() {}
    let after = it.item();
    assert_eq!((after.line(), after.column()), (-1, -1));
}

#[test]
fn test_prev() {
    let l = three_items();
    let mut it = l.new_iterator();
    assert!(!it.prev(), "prev before the first item");
    it.next();
    assert!(!it.prev(), "prev at the first item");
    it.next();
    assert_eq!(it.item().val(), "b");
    assert!(it.prev());
    assert_eq!(it.item().val(), "a");
    assert_eq!(it.save(), 0);
}

#[test]
fn test_save_restore() {
    let l = three_items();
    let mut it = l.new_iterator();
    assert_eq!(it.save(), -1);
    it.next();
    let pos = it.save();
    it.next();
    it.next();
    assert_eq!(it.item().val(), "c");
    it.restore(pos);
    assert_eq!(it.item().val(), "a");
    it.restore(-1);
    assert!(it.next());
    assert_eq!(it.item().val(), "a");
    it.restore(4);
    assert!(!it.next());
}

#[test]
#[should_panic]
fn test_restore_past_end_panics() {
    let l = three_items();
    let mut it = l.new_iterator();
    it.restore(5);
}

#[test]
#[should_panic]
fn test_restore_before_start_panics() {
    let l = three_items();
    let mut it = l.new_iterator();
    it.restore(-2);
}

#[test]
fn test_peek() {
    let l = three_items();
    let mut it = l.new_iterator();
    let items = it.peek(2).expect("two items ahead");
    assert_eq!(
        items.iter().map(|i| i.val()).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(it.save(), -1, "peek does not consume");
    it.next();
    let items = it.peek(3).expect("three items ahead");
    assert_eq!(
        items.iter().map(|i| i.val()).collect::<Vec<_>>(),
        vec!["b", "c", ""]
    );
    assert!(it.peek(0).expect("nothing ahead").is_empty());
    let err = it.peek(4).unwrap_err();
    assert_eq!(err.0, LexErrorEnum::PeekOutOfRange);
    assert_eq!(err.to_string(), "Out of range for peek");
}

#[test]
fn test_peek_one() {
    let l = three_items();
    let mut it = l.new_iterator();
    assert_eq!(it.peek_one().map(|i| i.val()), Some("a"));
    while it.next() {
        if it.item().typ() == ITEM_EOF {
            assert!(it.peek_one().is_none());
        }
    }
    assert!(it.peek_one().is_none());
}

#[test]
fn test_errorf_uses_next_item() {
    let mut l = Lexer::new("a\n  b");
    l.next();
    l.emit(ITEM_WORD);
    l.next();
    l.next();
    l.next();
    l.ignore();
    l.next();
    l.emit(ITEM_WORD);
    let mut it = l.new_iterator();
    it.next();
    assert_eq!(
        it.errorf("Expected {}").to_string(),
        "line 2 column 2: Expected {}"
    );
    it.next();
    assert_eq!(
        it.errorf(format_args!("Got {}", 3)).to_string(),
        "line -1 column -1: Got 3"
    );
}

#[test]
fn test_items_outlive_iterator_borrow() {
    let l = three_items();
    let mut it = l.new_iterator();
    it.next();
    let first = it.item();
    it.next();
    let second = it.item();
    // Both items borrow the lexer, not the iterator, so advancing does not invalidate them.
    assert_eq!((first.val(), second.val()), ("a", "b"));
}

#[test]
fn test_iterators_are_independent() {
    let l = three_items();
    let mut a = l.new_iterator();
    let mut b = l.new_iterator();
    a.next();
    a.next();
    b.next();
    assert_eq!((a.item().val(), b.item().val()), ("b", "a"));
}
