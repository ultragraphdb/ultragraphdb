/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{ITEM_EOF, ItemType, Lexer, StateFn};

fn lex_nothing(_l: &mut Lexer) -> Option<StateFn> {
    None
}

#[test]
fn test_new_starts_at_line_one() {
    let l = Lexer::new("abc");
    assert_eq!(l.input(), "abc");
    assert_eq!((l.start(), l.pos()), (0, 0));
    assert_eq!((l.line(), l.column()), (1, 0));
    assert_eq!((l.depth(), l.block_depth(), l.arg_depth()), (0, 0, 0));
    assert!(l.mode().is_none());
}

#[test]
fn test_reset_clears_state() {
    let mut l = Lexer::new("ab\ncd");
    l.set_depth(2);
    l.set_block_depth(1);
    l.set_arg_depth(3);
    l.set_mode(StateFn::new(lex_nothing));
    l.next();
    l.next();
    l.next();
    l.emit(ItemType::new(5));
    l.next();
    assert_eq!((l.line(), l.pos()), (2, 4));

    l.reset("xyz");
    assert_eq!(l.input(), "xyz");
    assert_eq!((l.start(), l.pos()), (0, 0));
    assert_eq!((l.line(), l.column()), (1, 0));
    assert_eq!((l.depth(), l.block_depth(), l.arg_depth()), (0, 0, 0));
    assert!(l.mode().is_none());
    let mut it = l.new_iterator();
    assert!(!it.next(), "reset drops the items");
    // The width stack is cleared too: the first rune of the new input backs up cleanly.
    assert_eq!(l.next(), Some('x'));
    l.backup();
    assert_eq!(l.pos(), 0);
}

#[test]
fn test_depth_setters() {
    let mut l = Lexer::new("");
    l.set_depth(l.depth() + 1);
    l.set_depth(l.depth() - 2);
    assert_eq!(l.depth(), -1);
    l.set_arg_depth(l.arg_depth() + 1);
    assert_eq!(l.arg_depth(), 1);
    l.set_block_depth(l.block_depth() + 4);
    assert_eq!(l.block_depth(), 4);
}

#[test]
fn test_input_outlives_the_lexer_borrow() {
    let mut l = Lexer::new("query rest");
    while l.peek() != Some(' ') {
        l.next();
    }
    // The word borrows the input, not the lexer, so the lexer stays usable while it is held.
    let word = &l.input()[l.start()..l.pos()];
    l.emit(ITEM_EOF);
    assert_eq!(word, "query");
}
