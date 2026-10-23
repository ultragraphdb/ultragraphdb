/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{ItemType, Lexer};

fn is_digit(r: char) -> bool {
    r.is_ascii_digit()
}

#[test]
fn test_accept_run_returns_last_accepted_rune() {
    let mut l = Lexer::new("123x");
    assert_eq!(l.accept_run(is_digit), Some('3'));
    assert_eq!(l.pos(), 3);
    assert_eq!(l.peek(), Some('x'));
}

#[test]
fn test_accept_run_none_accepted() {
    let mut l = Lexer::new("x1");
    assert_eq!(l.accept_run(is_digit), None);
    assert_eq!(l.pos(), 0);
}

#[test]
fn test_accept_run_to_eof() {
    let mut l = Lexer::new("42");
    assert_eq!(l.accept_run(is_digit), Some('2'));
    assert_eq!(l.pos(), 2);
    // The EOF read was backed up, so the next read is EOF again and the stack holds the two digits.
    assert_eq!(l.next(), None);
    l.backup();
    l.backup();
    assert_eq!(l.pos(), 1);
}

#[test]
fn test_accept_run_with_closure() {
    let stop = '=';
    let mut l = Lexer::new("key=value");
    l.accept_run(|r| r != stop);
    assert_eq!(&l.input()[l.start()..l.pos()], "key");
}

#[test]
fn test_accept_run_rec_reads_ahead() {
    // A rune checker that accepts "\\" only when a "!" follows, consuming both.
    fn bang_escape(r: char, l: &mut Lexer) -> bool {
        if r != '\\' {
            return r.is_ascii_alphabetic();
        }
        if l.next() == Some('!') {
            return true;
        }
        l.backup();
        false
    }
    let mut l = Lexer::new("ab\\!c\\d");
    l.accept_run_rec(bang_escape);
    assert_eq!(&l.input()[l.start()..l.pos()], "ab\\!c");
}

#[test]
fn test_accept_until() {
    let mut l = Lexer::new("abc\"rest");
    l.accept_until(|r| r == '"');
    assert_eq!(l.pos(), 3);
    assert_eq!(l.peek(), Some('"'));

    let mut l = Lexer::new("abc");
    l.accept_until(|r| r == '"');
    assert_eq!(l.pos(), 3);
}

#[test]
fn test_accept_run_times() {
    let mut l = Lexer::new("12345");
    assert_eq!(l.accept_run_times(is_digit, 3), 3);
    // As in Go, the third rune is backed up after a full count.
    assert_eq!(l.pos(), 2);

    let mut l = Lexer::new("12x45");
    assert_eq!(l.accept_run_times(is_digit, 4), 2);
    assert_eq!(l.pos(), 2);

    let mut l = Lexer::new("1");
    assert_eq!(l.accept_run_times(is_digit, 4), 1);
    assert_eq!(l.pos(), 1);
}

#[test]
fn test_ignore_run() {
    let mut l = Lexer::new("  \t x");
    l.ignore_run(|r| r == ' ' || r == '\t');
    assert_eq!((l.start(), l.pos(), l.column()), (4, 4, 4));
    l.next();
    l.emit(ItemType::new(5));
    let mut it = l.new_iterator();
    it.next();
    assert_eq!((it.item().val(), it.item().column()), ("x", 4));
}
