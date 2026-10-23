/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{ITEM_EOF, ItemType, Lexer, StateFn};

const ITEM_A: ItemType = ItemType::new(5);
const ITEM_B: ItemType = ItemType::new(6);

const LEX_A: StateFn = StateFn::new(lex_a);

// Two state functions handing control to each other, one rune at a time.
fn lex_a(l: &mut Lexer) -> Option<StateFn> {
    match l.next() {
        None => {
            l.emit(ITEM_EOF);
            None
        }
        Some(_) => {
            l.emit(ITEM_A);
            Some(StateFn::new(lex_b))
        }
    }
}

fn lex_b(l: &mut Lexer) -> Option<StateFn> {
    match l.next() {
        None => {
            l.emit(ITEM_EOF);
            None
        }
        Some(_) => {
            l.emit(ITEM_B);
            Some(LEX_A)
        }
    }
}

#[test]
fn test_state_functions_alternate() {
    let mut l = Lexer::new("xyz");
    l.run(LEX_A);
    let mut it = l.new_iterator();
    let mut got = Vec::new();
    while it.next() {
        got.push((it.item().typ(), it.item().val().to_string()));
    }
    assert_eq!(
        got,
        vec![
            (ITEM_A, "x".to_string()),
            (ITEM_B, "y".to_string()),
            (ITEM_A, "z".to_string()),
            (ITEM_EOF, String::new()),
        ]
    );
}

#[test]
fn test_state_fn_is_copy() {
    let f = LEX_A;
    let g = f;
    let mut l = Lexer::new("");
    l.run(f).run(g);
    // Each run emitted one EOF.
    let mut it = l.new_iterator();
    assert!(it.next());
    assert!(it.next());
    assert!(!it.next());
}
