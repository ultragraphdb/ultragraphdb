/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/rdf_state.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The lexer states of an RDF N-Quad line.

use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_core_lex::{
    ITEM_EOF, ItemType, Lexer, StateFn, has_u_chars, has_x_chars, iri_ref, is_end_of_line,
    is_esc_char,
};

// The item types of an RDF N-Quad.
/// plain text
pub(crate) const ITEM_TEXT: ItemType = ItemType::new(5);
/// subject, 6
pub(crate) const ITEM_SUBJECT: ItemType = ItemType::new(6);
/// predicate, 7
pub(crate) const ITEM_PREDICATE: ItemType = ItemType::new(7);
/// object, 8
pub(crate) const ITEM_OBJECT: ItemType = ItemType::new(8);
/// label, 9
pub(crate) const ITEM_LABEL: ItemType = ItemType::new(9);
/// literal, 10
pub(crate) const ITEM_LITERAL: ItemType = ItemType::new(10);
/// language, 11
pub(crate) const ITEM_LANGUAGE: ItemType = ItemType::new(11);
/// object type, 12
pub(crate) const ITEM_OBJECT_TYPE: ItemType = ItemType::new(12);
/// end with dot, 13
pub(crate) const ITEM_VALID_END: ItemType = ItemType::new(13);
/// comment, 14
pub(crate) const ITEM_COMMENT: ItemType = ItemType::new(14);
/// comma, 15
pub(crate) const ITEM_COMMA: ItemType = ItemType::new(15);
/// equal, 16
pub(crate) const ITEM_EQUAL: ItemType = ItemType::new(16);
/// '(', 17
pub(crate) const ITEM_LEFT_ROUND: ItemType = ItemType::new(17);
/// ')', 18
pub(crate) const ITEM_RIGHT_ROUND: ItemType = ItemType::new(18);
/// *, 19
pub(crate) const ITEM_STAR: ItemType = ItemType::new(19);
/// uid, 20
pub(crate) const ITEM_SUBJECT_FUNC: ItemType = ItemType::new(20);
/// uid, 21
pub(crate) const ITEM_OBJECT_FUNC: ItemType = ItemType::new(21);
/// 22
pub(crate) const ITEM_VAR_NAME: ItemType = ItemType::new(22);

// The depth while lexing an RDF N-Quad.
const AT_SUBJECT: i32 = 0;
const AT_PREDICATE: i32 = 1;
const AT_OBJECT: i32 = 2;
const AT_LABEL: i32 = 3;
const AT_FACET: i32 = 4;

const LS_THAN: char = '<';
const UNDERSCORE: char = '_';
const COLON: char = ':';
const DASH: char = '-';
const QUOTE: char = '"';
const HASH: char = '#';
const DOT: char = '.';
const AT: char = '@';
const CARET: char = '^';
const LEFT_ROUND: char = '(';
const RIGHT_ROUND: char = ')';
const COMMA: char = ',';
const EQUAL: char = '=';

pub(crate) const LEX_TEXT: StateFn = StateFn::new(lex_text);
const LEX_SUBJECT: StateFn = StateFn::new(lex_subject);
const LEX_PREDICATE: StateFn = StateFn::new(lex_predicate);
const LEX_OBJECT: StateFn = StateFn::new(lex_object);
const LEX_LABEL: StateFn = StateFn::new(lex_label);
const LEX_FACETS: StateFn = StateFn::new(lex_facets);
const LEX_COMMENT: StateFn = StateFn::new(lex_comment);
const LEX_VARIABLE: StateFn = StateFn::new(lex_variable);

/// A rune as Go's `%c` prints it: the end of the input (`EOF`, -1) as U+FFFD.
fn go_c(r: Option<char>) -> char {
    r.unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// Inspects the next rune and calls the appropriate state.
fn lex_text(l: &mut Lexer<'_>) -> Option<StateFn> {
    loop {
        let r = l.next();
        match r {
            Some(LS_THAN | UNDERSCORE) => {
                let next = match l.depth() {
                    AT_SUBJECT => LEX_SUBJECT,
                    AT_PREDICATE => LEX_PREDICATE,
                    AT_OBJECT => LEX_OBJECT,
                    AT_LABEL => LEX_LABEL,
                    _ => return l.errorf(format!("Invalid input: {} at lexText", go_c(r))),
                };
                l.backup();
                l.emit(ITEM_TEXT); // emit whatever we have so far.
                return Some(next);
            }
            Some(QUOTE) => {
                if l.depth() != AT_OBJECT {
                    return l.errorf("Invalid quote for non-object.");
                }
                l.backup();
                l.emit(ITEM_TEXT);
                return Some(LEX_OBJECT);
            }
            Some(HASH) => {
                if l.depth() != AT_SUBJECT {
                    return l.errorf(format!("Invalid input: {} at lexText", go_c(r)));
                }
                return Some(LEX_COMMENT);
            }
            Some('*') => {
                l.set_depth(l.depth() + 1);
                l.emit(ITEM_STAR);
            }
            Some(LEFT_ROUND) => {
                if l.depth() > AT_OBJECT {
                    l.backup();
                    l.emit(ITEM_TEXT);
                    return Some(LEX_FACETS);
                }
                return l.errorf(format!("Invalid input: {} at Facet", go_c(r)));
            }
            None => {
                l.emit(ITEM_EOF);
                return None;
            }
            Some(DOT) => {
                if l.depth() > AT_OBJECT {
                    l.emit(ITEM_VALID_END);
                    l.set_depth(AT_SUBJECT);
                }
            }
            // This should happen when there is either UID or Val function. Hence, we are just checking for u or v
            Some('u' | 'v') => {
                if l.depth() != AT_SUBJECT && l.depth() != AT_OBJECT {
                    return l.errorf(format!("Unexpected char '{}'", go_c(r)));
                }
                l.backup();
                l.emit(ITEM_TEXT);
                return Some(LEX_VARIABLE);
            }
            Some(c) if is_space(c) => {}
            // As in Dgraph, the error is recorded and lexing goes on.
            Some(_) => {
                l.errorf(format!("Invalid input: {} at lexText", go_c(r)));
            }
        }
    }
}

/// Assumes that caller has consumed initial '<'
fn lex_iri_ref(l: &mut Lexer<'_>, styp: ItemType, sfn: StateFn) -> Option<StateFn> {
    match iri_ref(l, styp) {
        Ok(()) => Some(sfn),
        Err(e) => l.errorf(e),
    }
}

/// A uid node. `lex_text` reaches `lex_subject` only at `<` or `_`, so Dgraph never lexes one.
fn lex_uid_node(l: &mut Lexer<'_>, styp: ItemType, sfn: StateFn) -> Option<StateFn> {
    l.accept_until(is_space);
    let r = l.peek();
    if r.is_none() {
        return l.errorf("Unexpected end of uid subject");
    }
    let input = &l.input()[l.start()..l.pos()];
    if go_strconv::parse_uint(input, 0).is_err() {
        return l.errorf(format!("Unable to convert '{input}' to UID"));
    }
    if r.is_some_and(is_space) {
        l.emit(styp);
        return Some(sfn);
    }
    l.errorf(format!(
        "Invalid character '{}' found for UID node itemType: {styp}",
        go_c(r)
    ))
}

/// Assumes that caller has consumed '_'.
/// BLANK_NODE_LABEL ::= '_:' (PN_CHARS_U | [0-9]) ((PN_CHARS | '.')* PN_CHARS)?
fn lex_blank_node(l: &mut Lexer<'_>, styp: ItemType, sfn: StateFn) -> Option<StateFn> {
    let r = l.next();
    if r != Some(COLON) {
        return l.errorf(format!(
            "Invalid character after _. Expected :, found '{}'",
            go_c(r)
        ));
    }
    let Some(r) = l.next() else {
        return l.errorf("Unexpected end of subject");
    };
    if !(is_pn_chars_u(r) || r.is_ascii_digit()) {
        return l.errorf(format!("Invalid character in {styp} after _: , Got '{r}'"));
    }
    if l.accept_run(|r| r == DOT || is_pn_char(r)) == Some(DOT) {
        return l.errorf(format!("Can not end {styp} with '.'"));
    }
    match l.peek() {
        None => l.errorf(format!("Unexpected end of {styp}")),
        Some(r) if is_space(r) => {
            l.emit(styp);
            Some(sfn)
        }
        r => l.errorf(format!(
            "Invalid character '{}' found for itemType: {styp}",
            go_c(r)
        )),
    }
}

fn lex_subject(l: &mut Lexer<'_>) -> Option<StateFn> {
    match l.next() {
        // The subject is an IRI, so we lex till we encounter '>'.
        Some(LS_THAN) => {
            l.set_depth(l.depth() + 1);
            lex_iri_ref(l, ITEM_SUBJECT, LEX_TEXT)
        }
        // The subject represents a blank node.
        Some(UNDERSCORE) => {
            l.set_depth(l.depth() + 1);
            lex_blank_node(l, ITEM_SUBJECT, LEX_TEXT)
        }
        // See if its an uid
        _ => lex_uid_node(l, ITEM_SUBJECT, LEX_TEXT),
    }
}

fn lex_predicate(l: &mut Lexer<'_>) -> Option<StateFn> {
    let r = l.next();
    // The predicate can only be an IRI according to the spec.
    if r != Some(LS_THAN) {
        return l.errorf(format!("Invalid character in lexPredicate: '{}'", go_c(r)));
    }
    l.set_depth(l.depth() + 1);
    lex_iri_ref(l, ITEM_PREDICATE, LEX_TEXT)
}

fn lex_language(l: &mut Lexer<'_>) -> Option<StateFn> {
    if l.next() != Some(AT) {
        return l.errorf("Expected @ prefix for lexLanguage");
    }
    l.ignore();
    let r = l.next();
    if !r.is_some_and(is_lang_tag_prefix) {
        return l.errorf(format!("Invalid language tag prefix: '{}'", go_c(r)));
    }
    if l.accept_run(is_lang_tag) == Some(DASH) {
        return l.errorf("Invalid character - at the end of language literal.");
    }
    l.emit(ITEM_LANGUAGE);
    Some(LEX_TEXT)
}

/// Assumes '"' has already been encountered.
/// literal ::= STRING_LITERAL_QUOTE ('^^' IRIREF | LANGTAG)?
/// STRING_LITERAL_QUOTE ::= '"' ([^#x22#x5C#xA#xD] | ECHAR | UCHAR)* '"'
fn lex_literal(l: &mut Lexer<'_>) -> Option<StateFn> {
    loop {
        let mut r = l.next();
        if r == Some('\u{005c}') {
            // backslash
            r = l.next();
            if is_esc_char(r) || has_u_chars(r, l) || has_x_chars(r, l) {
                continue; // This would skip over the escaped rune.
            }
            return l.errorf(format!(
                "Invalid escape character : '{}' in literal",
                go_c(r)
            ));
        }
        if matches!(r, Some('\u{5c}' | '\u{a}' | '\u{d}')) {
            // 0x22 ('"') is endLiteral
            return l.errorf(format!("Invalid character '{}' in literal.", go_c(r)));
        }
        if r.is_none_or(is_end_literal) {
            break;
        }
    }
    l.emit(ITEM_LITERAL);
    l.set_depth(l.depth() + 1);
    match l.peek() {
        Some(AT) => lex_language(l),
        Some(CARET) => lex_object_type(l),
        _ => Some(LEX_TEXT),
    }
}

fn lex_object_type(l: &mut Lexer<'_>) -> Option<StateFn> {
    if l.next() != Some(CARET) {
        return l.errorf("Expected ^ for lexObjectType");
    }
    if l.next() != Some(CARET) {
        return l.errorf("Expected ^^ for lexObjectType");
    }
    l.ignore();
    if l.next() != Some(LS_THAN) {
        return l.errorf("Expected < for lexObjectType");
    }
    lex_iri_ref(l, ITEM_OBJECT_TYPE, LEX_TEXT)
}

fn lex_object(l: &mut Lexer<'_>) -> Option<StateFn> {
    // The object can be an IRI, blank node, literal.
    match l.next() {
        Some(LS_THAN) => {
            l.set_depth(l.depth() + 1);
            lex_iri_ref(l, ITEM_OBJECT, LEX_TEXT)
        }
        Some(UNDERSCORE) => {
            l.set_depth(l.depth() + 1);
            lex_blank_node(l, ITEM_OBJECT, LEX_TEXT)
        }
        Some(QUOTE) => lex_literal(l),
        r => l.errorf(format!("Invalid char: '{}' at lexObject", go_c(r))),
    }
}

fn lex_label(l: &mut Lexer<'_>) -> Option<StateFn> {
    // Graph label can either be an IRI or a blank node according to spec.
    match l.next() {
        Some(LS_THAN) => {
            l.set_depth(l.depth() + 1);
            lex_iri_ref(l, ITEM_LABEL, LEX_TEXT)
        }
        Some(UNDERSCORE) => {
            l.set_depth(l.depth() + 1);
            lex_blank_node(l, ITEM_LABEL, LEX_TEXT)
        }
        r => l.errorf(format!("Invalid char: '{}' at lexLabel", go_c(r))),
    }
}

/// Lexes the key-value pairs of facets, e.g.
/// `( key1 = "value1", key2=13, key3=, key4 =2.4, key5=2006-01-02T15:04:05, key6=2006-01-02 )`.
fn lex_facets(l: &mut Lexer<'_>) -> Option<StateFn> {
    let r = l.next();
    if r != Some(LEFT_ROUND) {
        return l.errorf(format!("Expected '(' but found '{}' at Facet.", go_c(r)));
    }
    l.emit(ITEM_LEFT_ROUND);

    // we can come here from the lexObject also; so setting to ahead of atFacet explicitly
    l.set_depth(AT_FACET + 1);

    loop {
        match l.next() {
            Some(r) if is_space(r) => l.ignore(),
            Some(EQUAL) => l.emit(ITEM_EQUAL),
            Some(COMMA) => l.emit(ITEM_COMMA),
            Some(RIGHT_ROUND) => {
                l.emit(ITEM_RIGHT_ROUND);
                break;
            }
            None => {
                l.emit(ITEM_EOF);
                return None;
            }
            Some(QUOTE) => {
                if let Err(e) = l.lex_quoted_string() {
                    return l.errorf(e);
                }
                l.emit(ITEM_TEXT);
            }
            Some(_) => {
                l.accept_run(|r| r != EQUAL && !is_space(r) && r != RIGHT_ROUND && r != COMMA);
                l.emit(ITEM_TEXT);
            }
        }
    }
    Some(LEX_TEXT)
}

/// Lexes a comment text.
fn lex_comment(l: &mut Lexer<'_>) -> Option<StateFn> {
    l.backup();
    while l.next().is_some_and(|r| !is_end_of_line(r)) {}
    l.emit(ITEM_COMMENT);
    l.emit(ITEM_EOF);
    None // Stop the run loop.
}

fn lex_variable(l: &mut Lexer<'_>) -> Option<StateFn> {
    let function_name = if l.next() == Some('v') { "val" } else { "uid" };
    l.backup();

    for c in function_name.chars() {
        let r = l.next();
        if r != Some(c) {
            return l.errorf(format!(
                "Unexpected char '{}' when parsing uid keyword",
                go_c(r)
            ));
        }
    }

    if l.depth() == AT_OBJECT {
        l.emit(ITEM_OBJECT_FUNC);
    } else if l.depth() == AT_SUBJECT {
        l.emit(ITEM_SUBJECT_FUNC);
    }
    l.ignore_run(is_space);

    let r = l.next();
    if r != Some('(') {
        return l.errorf(format!(
            "Expected '(' after uid keyword, found: '{}'",
            go_c(r)
        ));
    }

    l.emit(ITEM_LEFT_ROUND);
    l.ignore_run(is_space);

    // Dgraph accepts every character in a variable name except space and right bracket: a variable must be used
    // once in a query block before it is used here, and the count of used and defined variables is checked.
    if l.accept_run(|r| !(is_space(r) || r == ')')).is_none() {
        return l.errorf("Unexpected end of input while reading variable name");
    }
    l.emit(ITEM_VAR_NAME);
    l.ignore_run(is_space);

    let r = l.next();
    if r != Some(')') {
        return l.errorf(format!(
            "Expected ')' while reading function found: '{}'",
            go_c(r)
        ));
    }
    l.emit(ITEM_RIGHT_ROUND);
    l.set_depth(l.depth() + 1);

    Some(LEX_TEXT)
}

/// Whether the rune is a tab or space.
fn is_space(r: char) -> bool {
    r == '\u{0009}' || r == '\u{0020}'
}

fn is_end_literal(r: char) -> bool {
    r == QUOTE || r == '\u{000d}' || r == '\u{000a}'
}

fn is_lang_tag_prefix(r: char) -> bool {
    r.is_ascii_alphabetic()
}

/// Whether the rune is allowed in a language tag by the RDF spec.
fn is_lang_tag(r: char) -> bool {
    is_lang_tag_prefix(r) || r == DASH || r.is_ascii_digit()
}

/// PN_CHARS_BASE ::= [A-Z] | [a-z] | [#x00C0-#x00D6] | [#x00D8-#x00F6] | [#x00F8-#x02FF] | [#x0370-#x037D] |
/// [#x037F-#x1FFF] | [#x200C-#x200D] | [#x2070-#x218F] | [#x2C00-#x2FEF] | [#x3001-#xD7FF] | [#xF900-#xFDCF] |
/// [#xFDF0-#xFFFD] | [#x10000-#xEFFFF]
fn is_pn_chars_base(r: char) -> bool {
    matches!(r,
        'a'..='z'
        | 'A'..='Z'
        | '\u{C0}'..='\u{D6}'
        | '\u{D8}'..='\u{F6}'
        | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}'
        | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}'
        | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}'
        | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

/// PN_CHARS_U ::= PN_CHARS_BASE | '_' | ':'
fn is_pn_chars_u(r: char) -> bool {
    r == UNDERSCORE || r == COLON || is_pn_chars_base(r)
}

/// PN_CHARS ::= PN_CHARS_U | '-' | [0-9] | #x00B7 | [#x0300-#x036F] | [#x203F-#x2040]
fn is_pn_char(r: char) -> bool {
    matches!(r, DASH | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
        || is_pn_chars_u(r)
}
