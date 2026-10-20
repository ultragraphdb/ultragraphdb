/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A small lexer in the shape of Dgraph's DQL lexer (dql/state.go): braces tracked by `depth`, round brackets by
//! `arg_depth`, the state to resume after a name, an IRI or a comment held in `mode`, and quoted strings.

use ultragraphdb_core_lex::{
    ITEM_EOF, ITEM_ERROR, ItemType, LexErrorEnum, Lexer, StateFn, iri_ref, is_end_of_line,
};

const ITEM_TEXT: ItemType = ItemType::new(5);
const ITEM_LEFT_CURL: ItemType = ItemType::new(6);
const ITEM_RIGHT_CURL: ItemType = ItemType::new(7);
const ITEM_NAME: ItemType = ItemType::new(9);
const ITEM_LEFT_ROUND: ItemType = ItemType::new(11);
const ITEM_RIGHT_ROUND: ItemType = ItemType::new(12);
const ITEM_COMMA: ItemType = ItemType::new(25);

fn is_space(r: char) -> bool {
    r == '\t' || r == ' '
}

fn is_name_suffix(r: char) -> bool {
    r.is_ascii_alphanumeric() || r == '_' || r == '.'
}

fn lex_top_level(l: &mut Lexer) -> Option<StateFn> {
    l.set_mode(StateFn::new(lex_top_level));
    loop {
        match l.next() {
            Some('{') => {
                l.set_depth(l.depth() + 1);
                l.emit(ITEM_LEFT_CURL);
                return Some(StateFn::new(lex_query));
            }
            Some('}') => return l.errorf("Too many right curl"),
            None => break,
            Some('#') => return Some(StateFn::new(lex_comment)),
            Some(r) if is_space(r) || is_end_of_line(r) => l.ignore(),
            Some(_) => {}
        }
    }
    if l.pos() > l.start() {
        l.emit(ITEM_TEXT);
    }
    l.emit(ITEM_EOF);
    None
}

fn lex_query(l: &mut Lexer) -> Option<StateFn> {
    l.set_mode(StateFn::new(lex_query));
    loop {
        match l.next() {
            Some('}') => {
                l.set_depth(l.depth() - 1);
                l.emit(ITEM_RIGHT_CURL);
                if l.depth() == 0 {
                    return Some(StateFn::new(lex_top_level));
                }
            }
            Some('{') => {
                l.set_depth(l.depth() + 1);
                l.emit(ITEM_LEFT_CURL);
            }
            Some('(') => {
                l.emit(ITEM_LEFT_ROUND);
                l.set_arg_depth(l.arg_depth() + 1);
                return Some(StateFn::new(lex_func_or_arg));
            }
            Some('<') => return Some(StateFn::new(lex_iri_ref)),
            Some('#') => return Some(StateFn::new(lex_comment)),
            None => return l.errorf("Unclosed action"),
            Some(r) if is_space(r) || is_end_of_line(r) => l.ignore(),
            Some(r) if r.is_ascii_alphabetic() => return Some(StateFn::new(lex_name)),
            Some(r) => return l.errorf(format_args!("Unrecognized character in lexText: {r:?}")),
        }
    }
}

fn lex_func_or_arg(l: &mut Lexer) -> Option<StateFn> {
    l.set_mode(StateFn::new(lex_func_or_arg));
    loop {
        match l.next() {
            Some('(') => {
                l.emit(ITEM_LEFT_ROUND);
                l.set_arg_depth(l.arg_depth() + 1);
            }
            Some(')') => {
                l.set_arg_depth(l.arg_depth() - 1);
                l.emit(ITEM_RIGHT_ROUND);
                if l.arg_depth() == 0 {
                    return Some(StateFn::new(lex_query));
                }
            }
            Some('"') => {
                if let Err(err) = l.lex_quoted_string() {
                    return l.errorf(err);
                }
                l.emit(ITEM_NAME);
            }
            Some(',') => l.emit(ITEM_COMMA),
            None => return l.errorf("Unclosed Brackets"),
            Some(r) if is_space(r) || is_end_of_line(r) => l.ignore(),
            Some(r) if r.is_ascii_alphanumeric() => return Some(StateFn::new(lex_name)),
            Some(r) => {
                return l.errorf(format_args!("Unrecognized character inside a func: {r:?}"));
            }
        }
    }
}

fn lex_name(l: &mut Lexer) -> Option<StateFn> {
    l.accept_run(is_name_suffix);
    l.emit(ITEM_NAME);
    l.mode()
}

fn lex_iri_ref(l: &mut Lexer) -> Option<StateFn> {
    if let Err(err) = iri_ref(l, ITEM_NAME) {
        return l.errorf(err);
    }
    l.mode()
}

fn lex_comment(l: &mut Lexer) -> Option<StateFn> {
    loop {
        match l.next() {
            Some(r) if is_end_of_line(r) => {
                l.ignore();
                return l.mode();
            }
            None => break,
            Some(_) => {}
        }
    }
    l.ignore();
    l.emit(ITEM_EOF);
    l.mode()
}

fn lex(input: &str) -> Vec<(ItemType, String, i32, i32)> {
    let mut l = Lexer::new(input);
    l.run(StateFn::new(lex_top_level));
    let mut it = l.new_iterator();
    let mut out = Vec::new();
    while it.next() {
        let item = it.item();
        out.push((
            item.typ(),
            item.val().to_string(),
            item.line(),
            item.column(),
        ));
    }
    out
}

fn item(typ: ItemType, val: &str, line: i32, column: i32) -> (ItemType, String, i32, i32) {
    (typ, val.to_string(), line, column)
}

#[test]
fn test_nested_query() {
    let got = lex("{\n  me(f(\"a\\\"b\", 3)) {\n    <http://x/y> # c\n    name\n  }\n}");
    assert_eq!(
        got,
        vec![
            item(ITEM_LEFT_CURL, "{", 1, 0),
            item(ITEM_NAME, "me", 2, 2),
            item(ITEM_LEFT_ROUND, "(", 2, 4),
            item(ITEM_NAME, "f", 2, 5),
            item(ITEM_LEFT_ROUND, "(", 2, 6),
            item(ITEM_NAME, "\"a\\\"b\"", 2, 7),
            item(ITEM_COMMA, ",", 2, 13),
            item(ITEM_NAME, "3", 2, 15),
            item(ITEM_RIGHT_ROUND, ")", 2, 16),
            item(ITEM_RIGHT_ROUND, ")", 2, 17),
            item(ITEM_LEFT_CURL, "{", 2, 19),
            item(ITEM_NAME, "http://x/y", 3, 5),
            item(ITEM_NAME, "name", 4, 4),
            item(ITEM_RIGHT_CURL, "}", 5, 2),
            item(ITEM_RIGHT_CURL, "}", 6, 0),
            item(ITEM_EOF, "", 6, 1),
        ]
    );
}

#[test]
fn test_unclosed_brace_is_an_error() {
    let mut l = Lexer::new("{\n  me {\n    name\n  }\n");
    l.run(StateFn::new(lex_top_level));
    assert_eq!(l.depth(), 1);
    let err = l.validate_result().unwrap_err();
    assert_eq!(
        err.to_string(),
        "while lexing {\n  me {\n    name\n  }\n at line 5 column 0: Unclosed action"
    );
    assert_eq!(err.kind(), &LexErrorEnum::Lexing(err.to_string()));
    let last = lex("{ me (");
    assert_eq!(last.last().map(|i| i.0), Some(ITEM_ERROR));
}

#[test]
fn test_validate_result_reports_the_first_error() {
    let mut l = Lexer::new("x");
    l.errorf("first");
    l.errorf("second");
    let err = l.validate_result().unwrap_err();
    assert_eq!(err.to_string(), "while lexing x at line 1 column 0: first");
}

#[test]
fn test_validate_result_ok() {
    let mut l = Lexer::new("{ a }");
    l.run(StateFn::new(lex_top_level));
    assert!(l.validate_result().is_ok());
    assert_eq!((l.depth(), l.arg_depth()), (0, 0));
}

#[test]
fn test_lexer_errors_carry_through_state_functions() {
    let got = lex("{ me(\"a\\q\") }");
    assert_eq!(
        got.last(),
        Some(&item(
            ITEM_ERROR,
            "while lexing { me(\"a\\q\") } at line 1 column 5: Not a valid escape char: 'q'",
            1,
            5
        ))
    );
    let got = lex("{ <a b> }");
    assert_eq!(
        got.last(),
        Some(&item(
            ITEM_ERROR,
            "while lexing { <a b> } at line 1 column 4: Unexpected character ' ' while parsing IRI",
            1,
            4
        ))
    );
}

#[test]
fn test_comment_at_eof_resumes_mode() {
    // The comment emits EOF and hands back to the top level, which emits EOF again, as the DQL lexer does.
    let got = lex("{ a } # done");
    let eofs = got.iter().filter(|i| i.0 == ITEM_EOF).count();
    assert_eq!(eofs, 2);
}

#[test]
fn test_reset_reuses_the_lexer() {
    let inputs = ["{ a }", "{ b { c } }"];
    let mut l = Lexer::new("");
    let mut counts = Vec::new();
    for input in inputs {
        l.reset(input);
        l.run(StateFn::new(lex_top_level));
        assert!(l.validate_result().is_ok());
        let mut it = l.new_iterator();
        let mut n = 0;
        while it.next() {
            n += 1;
        }
        counts.push(n);
    }
    assert_eq!(counts, vec![4, 7]);
}
