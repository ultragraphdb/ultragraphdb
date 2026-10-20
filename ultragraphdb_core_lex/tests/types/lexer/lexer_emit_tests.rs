/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_lex::{ITEM_EOF, ITEM_ERROR, ItemType, Lexer, StateFn};
use ultragraphdb_test_bdd::{load_golden, testdata_dir};

const ITEM_RUNE: ItemType = ItemType::new(5);

#[test]
fn test_emit_moves_start() {
    let mut l = Lexer::new("ab");
    l.next();
    l.emit(ITEM_RUNE);
    assert_eq!((l.start(), l.pos(), l.column()), (1, 1, 1));
    l.next();
    l.emit(ITEM_RUNE);
    let mut it = l.new_iterator();
    it.next();
    assert_eq!(it.item().val(), "a");
    it.next();
    assert_eq!((it.item().val(), it.item().column()), ("b", 1));
}

#[test]
fn test_emit_empty_item() {
    let mut l = Lexer::new("a");
    l.emit(ITEM_RUNE);
    let mut it = l.new_iterator();
    assert!(it.next());
    assert_eq!((it.item().typ(), it.item().val()), (ITEM_RUNE, ""));
}

#[test]
fn test_emit_skips_when_pos_is_before_start() {
    let mut l = Lexer::new("ab");
    l.next();
    l.ignore();
    l.backup();
    assert!(l.pos() < l.start());
    l.emit(ITEM_RUNE);
    l.emit(ITEM_ERROR);
    let mut it = l.new_iterator();
    assert!(!it.next());
    assert_eq!((l.start(), l.pos()), (1, 0));
}

#[test]
fn test_ignore_tracks_lines_and_columns() {
    let mut l = Lexer::new("é\r\n  x");
    for _ in 0..5 {
        l.next();
    }
    l.ignore();
    // "\r\n" is two line ends; the column counts bytes.
    assert_eq!((l.line(), l.column(), l.start()), (3, 2, 6));
}

#[test]
fn test_errorf_emits_an_error_item_and_stops() {
    let mut l = Lexer::new("a\nbc");
    l.next();
    l.next();
    l.ignore();
    l.next();
    let state = l.errorf(format_args!("Unrecognized character: {:?}", 'b'));
    assert!(state.is_none());
    let mut it = l.new_iterator();
    assert!(it.next());
    assert_eq!(it.item().typ(), ITEM_ERROR);
    assert_eq!(
        it.item().val(),
        "while lexing a\nbc at line 2 column 0: Unrecognized character: 'b'"
    );
    assert_eq!((it.item().line(), it.item().column()), (2, 0));
    // An error item does not move start.
    assert_eq!(l.start(), 2);
}

#[derive(Deserialize)]
struct PositionItem {
    typ: i32,
    val: String,
    string: String,
    errorf: String,
}

#[derive(Deserialize)]
struct PositionCase {
    input: String,
    items: Vec<PositionItem>,
    validate: String,
}

// The golden generator's state function: every rune an item of its own, spaces ignored, '!' an error.
fn lex_per_rune(l: &mut Lexer) -> Option<StateFn> {
    loop {
        match l.next() {
            None => {
                l.emit(ITEM_EOF);
                return None;
            }
            Some(' ') => l.ignore(),
            Some('!') => {
                let depth = l.depth();
                return l.errorf(format_args!("Unexpected '!' at depth {depth}"));
            }
            Some(_) => l.emit(ITEM_RUNE),
        }
    }
}

#[test]
fn test_positions_golden() {
    let golden = load_golden::<PositionCase>(
        &testdata_dir("ultragraphdb_core_lex").join("golden/lex_positions.json"),
    );
    assert!(!golden.cases().is_empty());
    for case in golden.cases() {
        let mut l = Lexer::new(&case.input);
        l.run(StateFn::new(lex_per_rune));
        let mut it = l.new_iterator();
        let mut n = 0;
        while it.next() {
            let item = it.item();
            let want = &case.items[n];
            assert_eq!(
                item.typ(),
                ItemType::new(want.typ),
                "input {:?} item {n}",
                case.input
            );
            assert_eq!(item.val(), want.val, "input {:?} item {n}", case.input);
            assert_eq!(
                item.to_string(),
                want.string,
                "input {:?} item {n}",
                case.input
            );
            assert_eq!(
                item.errorf(format_args!("Unexpected {} here", "x"))
                    .to_string(),
                want.errorf,
                "input {:?} item {n}",
                case.input
            );
            n += 1;
        }
        assert_eq!(n, case.items.len(), "input {:?}", case.input);
        let validate = match l.validate_result() {
            Ok(()) => String::new(),
            Err(e) => e.to_string(),
        };
        assert_eq!(validate, case.validate, "input {:?}", case.input);
    }
}
