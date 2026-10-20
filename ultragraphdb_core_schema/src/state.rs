/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/state.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The schema lexer's states and item types.

use ultragraphdb_core_lex::{ITEM_EOF, ItemType, Lexer, StateFn, iri_ref, is_end_of_line};

/// Plain text.
pub(crate) const ITEM_TEXT: ItemType = ItemType::new(5);
/// Number.
pub(crate) const ITEM_NUMBER: ItemType = ItemType::new(6);
/// Left curly bracket.
pub(crate) const ITEM_LEFT_CURL: ItemType = ItemType::new(7);
/// Right curly bracket.
pub(crate) const ITEM_RIGHT_CURL: ItemType = ItemType::new(8);
/// Colon.
pub(crate) const ITEM_COLON: ItemType = ItemType::new(9);
/// Left round bracket.
pub(crate) const ITEM_LEFT_ROUND: ItemType = ItemType::new(10);
/// Right round bracket.
pub(crate) const ITEM_RIGHT_ROUND: ItemType = ItemType::new(11);
/// `@`.
pub(crate) const ITEM_AT: ItemType = ItemType::new(12);
/// `,`.
pub(crate) const ITEM_COMMA: ItemType = ItemType::new(13);
/// Carriage return or line feed.
pub(crate) const ITEM_NEW_LINE: ItemType = ItemType::new(14);
/// `.`.
pub(crate) const ITEM_DOT: ItemType = ItemType::new(15);
/// `[`.
pub(crate) const ITEM_LEFT_SQUARE: ItemType = ItemType::new(16);
/// `]`.
pub(crate) const ITEM_RIGHT_SQUARE: ItemType = ItemType::new(17);
/// `!`.
pub(crate) const ITEM_EXCLAMATION_MARK: ItemType = ItemType::new(18);
/// A double quote, `"`; declared as in Go, never emitted.
#[allow(dead_code)]
pub(crate) const ITEM_QUOTE: ItemType = ItemType::new(19);
/// A quoted string, its quotes included.
pub(crate) const ITEM_QUOTED_TEXT: ItemType = ItemType::new(20);

/// The lexer's start state.
pub(crate) const LEX_TEXT: StateFn = StateFn::new(lex_text);

fn lex_text(l: &mut Lexer<'_>) -> Option<StateFn> {
    while let Some(r) = l.next() {
        match r {
            r if is_name_begin(r) => {
                l.backup();
                return Some(StateFn::new(lex_word));
            }
            r if is_space(r) => l.ignore(),
            r if is_end_of_line(r) => l.emit(ITEM_NEW_LINE),
            '.' => l.emit(ITEM_DOT),
            '#' => return Some(StateFn::new(lex_text_comment)),
            ',' => l.emit(ITEM_COMMA),
            '<' => {
                if let Err(err) = iri_ref(l, ITEM_TEXT) {
                    return l.errorf(format!("Invalid schema: {err}"));
                }
            }
            '{' => l.emit(ITEM_LEFT_CURL),
            '}' => l.emit(ITEM_RIGHT_CURL),
            '(' => l.emit(ITEM_LEFT_ROUND),
            ')' => l.emit(ITEM_RIGHT_ROUND),
            ':' => l.emit(ITEM_COLON),
            '@' => l.emit(ITEM_AT),
            '[' => l.emit(ITEM_LEFT_SQUARE),
            ']' => l.emit(ITEM_RIGHT_SQUARE),
            '!' => l.emit(ITEM_EXCLAMATION_MARK),
            // Predicates can start with _.
            '_' => return Some(StateFn::new(lex_word)),
            r if is_digit(r) => {
                let next_runes = l.peek_two();
                l.backup();
                if r == '0'
                    && next_runes[0].is_some_and(is_hex_separator)
                    && next_runes[1].is_some_and(is_hexadecimal)
                {
                    return Some(StateFn::new(lex_hex_number));
                }
                return Some(StateFn::new(lex_number));
            }
            '"' => {
                if let Err(err) = l.lex_quoted_string() {
                    return l.errorf(format!("Invalid schema: {err}"));
                }
                l.emit(ITEM_QUOTED_TEXT);
            }
            _ => {
                let unexpected = &l.input()[l.start()..l.pos()];
                return l.errorf(format!("Invalid schema. Unexpected {unexpected}"));
            }
        }
    }
    if l.pos() > l.start() {
        l.emit(ITEM_TEXT);
    }
    l.emit(ITEM_EOF);
    None
}

fn lex_word(l: &mut Lexer<'_>) -> Option<StateFn> {
    // The caller already checked is_name_begin, and absorbed one rune.
    l.accept_run(is_name_suffix);
    l.emit(ITEM_TEXT);
    Some(LEX_TEXT)
}

fn lex_number(l: &mut Lexer<'_>) -> Option<StateFn> {
    // The caller already checked is_digit, and absorbed one rune.
    l.accept_run(is_digit);
    l.emit(ITEM_NUMBER);
    Some(LEX_TEXT)
}

fn lex_hex_number(l: &mut Lexer<'_>) -> Option<StateFn> {
    // It satisfies 0[xX], then process the input as hexadecimal.
    l.next();
    l.next(); // Absorb 0[xX]
    l.accept_run(is_hexadecimal);
    l.emit(ITEM_NUMBER);
    Some(LEX_TEXT)
}

/// Lexes a comment inside a schema.
fn lex_text_comment(l: &mut Lexer<'_>) -> Option<StateFn> {
    loop {
        match l.next() {
            None => {
                l.ignore();
                l.emit(ITEM_EOF);
                break;
            }
            Some(r) if is_end_of_line(r) => {
                l.ignore();
                l.emit(ITEM_NEW_LINE);
                break;
            }
            Some(_) => {}
        }
    }
    Some(LEX_TEXT)
}

/// Whether the rune is an ASCII letter.
fn is_name_begin(r: char) -> bool {
    r.is_ascii_alphabetic()
}

fn is_name_suffix(r: char) -> bool {
    // '_', '.' and '-' are used by freebase.
    is_name_begin(r) || is_digit(r) || matches!(r, '_' | '.' | '-')
}

fn is_hexadecimal(r: char) -> bool {
    r.is_ascii_hexdigit()
}

fn is_hex_separator(r: char) -> bool {
    r == 'x' || r == 'X'
}

fn is_digit(r: char) -> bool {
    r.is_ascii_digit()
}

/// Whether the rune is a tab or a space.
fn is_space(r: char) -> bool {
    r == '\t' || r == ' '
}
