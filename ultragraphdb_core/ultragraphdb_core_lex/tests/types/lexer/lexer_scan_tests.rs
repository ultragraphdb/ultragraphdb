/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::Lexer;

#[test]
fn test_next_decodes_runes() {
    let mut l = Lexer::new("aé名😀");
    assert_eq!(l.next(), Some('a'));
    assert_eq!(l.pos(), 1);
    assert_eq!(l.next(), Some('é'));
    assert_eq!(l.pos(), 3);
    assert_eq!(l.next(), Some('名'));
    assert_eq!(l.pos(), 6);
    assert_eq!(l.next(), Some('😀'));
    assert_eq!(l.pos(), 10);
    assert_eq!(l.next(), None);
    assert_eq!(l.next(), None);
    assert_eq!(l.pos(), 10);
}

#[test]
fn test_backup_restores_widths_in_order() {
    // Go's own example: two one-byte runes, then a three-byte rune, backed up one at a time.
    let mut l = Lexer::new("ab名c");
    l.next();
    l.next();
    l.next();
    assert_eq!(l.pos(), 5);
    l.backup();
    assert_eq!(l.pos(), 2);
    l.backup();
    assert_eq!(l.pos(), 1);
    l.backup();
    assert_eq!(l.pos(), 0);
}

#[test]
fn test_backup_after_eof_is_zero_width() {
    let mut l = Lexer::new("é");
    l.next();
    assert_eq!(l.next(), None);
    assert_eq!(l.next(), None);
    l.backup();
    l.backup();
    assert_eq!(l.pos(), 2);
    l.backup();
    assert_eq!(l.pos(), 0);
}

#[test]
fn test_backup_on_empty_stack_does_nothing() {
    // Go's assertion exits the process with "Backup should not be called when the width tracker stack is empty".
    let mut l = Lexer::new("a");
    l.backup();
    assert_eq!(l.pos(), 0);
}

#[test]
fn test_peek() {
    let mut l = Lexer::new("é!");
    assert_eq!(l.peek(), Some('é'));
    assert_eq!(l.pos(), 0);
    l.next();
    assert_eq!(l.peek(), Some('!'));
    l.next();
    assert_eq!(l.peek(), None);
    assert_eq!(l.pos(), 3);
}

#[test]
fn test_peek_two() {
    let mut l = Lexer::new("0x");
    assert_eq!(l.peek_two(), [Some('0'), Some('x')]);
    assert_eq!(l.pos(), 0);
    l.next();
    assert_eq!(l.peek_two(), [Some('x'), None]);
    assert_eq!(l.pos(), 1);
    l.next();
    assert_eq!(l.peek_two(), [None, None]);
    assert_eq!(l.pos(), 2);
    // Peeking leaves the width stack balanced.
    l.backup();
    l.backup();
    assert_eq!(l.pos(), 0);
}

#[test]
fn test_backup_without_a_read_does_nothing() {
    // Go's assertion exits the process; the port has no read to undo and stays where it is.
    let mut l = Lexer::new("ab");
    l.backup();
    assert_eq!(l.pos(), 0);
    l.next();
    l.backup();
    l.backup();
    assert_eq!(l.pos(), 0);
    assert_eq!(l.next(), Some('a'));
}
