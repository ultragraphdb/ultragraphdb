/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/state.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The DQL lexer's states and item types.

use ultragraphdb_core_lex::{ITEM_EOF, ItemType, Lexer, StateFn, iri_ref, is_end_of_line};

use crate::utils::go_quote::unicode_hash;

const LEFT_CURL: char = '{';
const RIGHT_CURL: char = '}';
const LEFT_ROUND: char = '(';
const RIGHT_ROUND: char = ')';
const LEFT_SQUARE: char = '[';
const RIGHT_SQUARE: char = ']';
const PERIOD: char = '.';
const COMMA: char = ',';
const SLASH: char = '/';
const EQUAL: char = '=';
const QUOTE: char = '"';
const AT: char = '@';
const COLON: char = ':';
const LS_THAN: char = '<';
pub(crate) const STAR: char = '*';

/// Plain text.
pub(crate) const ITEM_TEXT: ItemType = ItemType::new(5);
/// Left curly bracket.
pub(crate) const ITEM_LEFT_CURL: ItemType = ItemType::new(6);
/// Right curly bracket.
pub(crate) const ITEM_RIGHT_CURL: ItemType = ItemType::new(7);
/// Equals to symbol.
pub(crate) const ITEM_EQUAL: ItemType = ItemType::new(8);
/// Names.
pub(crate) const ITEM_NAME: ItemType = ItemType::new(9);
/// Operation type.
pub(crate) const ITEM_OP_TYPE: ItemType = ItemType::new(10);
/// Left round bracket.
pub(crate) const ITEM_LEFT_ROUND: ItemType = ItemType::new(11);
/// Right round bracket.
pub(crate) const ITEM_RIGHT_ROUND: ItemType = ItemType::new(12);
/// Colon.
pub(crate) const ITEM_COLON: ItemType = ItemType::new(13);
/// `@`.
pub(crate) const ITEM_AT: ItemType = ItemType::new(14);
/// `.`.
pub(crate) const ITEM_PERIOD: ItemType = ItemType::new(15);
/// `$`.
pub(crate) const ITEM_DOLLAR: ItemType = ItemType::new(16);
/// `/`.
pub(crate) const ITEM_REGEX: ItemType = ItemType::new(17);
/// Mutation operation (set, delete).
pub(crate) const ITEM_MUTATION_OP: ItemType = ItemType::new(18);
/// Mutation operation content.
pub(crate) const ITEM_MUTATION_OP_CONTENT: ItemType = ItemType::new(19);
/// Mutation upsert block.
pub(crate) const ITEM_UPSERT_BLOCK: ItemType = ItemType::new(20);
/// Upsert block op (query, mutate).
pub(crate) const ITEM_UPSERT_BLOCK_OP: ItemType = ItemType::new(21);
/// Upsert block operations' content.
pub(crate) const ITEM_UPSERT_BLOCK_OP_CONTENT: ItemType = ItemType::new(22);
pub(crate) const ITEM_LEFT_SQUARE: ItemType = ItemType::new(23);
pub(crate) const ITEM_RIGHT_SQUARE: ItemType = ItemType::new(24);
pub(crate) const ITEM_COMMA: ItemType = ItemType::new(25);
pub(crate) const ITEM_MATH_OP: ItemType = ItemType::new(26);
pub(crate) const ITEM_STAR: ItemType = ItemType::new(27);

/// The lexer's start state for a whole request: queries, mutations, schema and upsert blocks.
pub const LEX_TOP_LEVEL: StateFn = StateFn::new(lex_top_level);
/// The lexer's state inside a query block.
pub const LEX_QUERY: StateFn = StateFn::new(lex_query);

fn word<'a>(l: &Lexer<'a>) -> &'a str {
    &l.input()[l.start()..l.pos()]
}

fn is_space_or_eol(r: char) -> bool {
    is_space(r) || is_end_of_line(r)
}

/// Lexes the upsert block.
fn lex_upsert_block(l: &mut Lexer<'_>) -> Option<StateFn> {
    l.set_mode(StateFn::new(lex_upsert_block));
    loop {
        match l.next() {
            Some(RIGHT_CURL) => {
                l.set_block_depth(l.block_depth() - 1);
                l.emit(ITEM_RIGHT_CURL);
                if l.block_depth() == 0 {
                    return Some(LEX_TOP_LEVEL);
                }
            }
            Some(LEFT_CURL) => {
                l.set_block_depth(l.block_depth() + 1);
                l.emit(ITEM_LEFT_CURL);
            }
            Some(r) if is_space_or_eol(r) => l.ignore(),
            Some(r) if is_name_begin(r) => return Some(StateFn::new(lex_name_upsert_op)),
            Some('#') => return Some(StateFn::new(lex_comment)),
            None => return l.errorf("Unclosed upsert block"),
            r => {
                return l.errorf(format!(
                    "Unrecognized character in upsert block: {}",
                    unicode_hash(r)
                ));
            }
        }
    }
}

/// Lexes the operation names inside the upsert block.
fn lex_name_upsert_op(l: &mut Lexer<'_>) -> Option<StateFn> {
    // The caller already checked isNameBegin, and absorbed one rune.
    l.accept_run(is_name_suffix);
    match word(l) {
        "query" | "fragment" => {
            l.emit(ITEM_UPSERT_BLOCK_OP);
            Some(StateFn::new(lex_block_content))
        }
        "mutation" => {
            l.emit(ITEM_UPSERT_BLOCK_OP);
            Some(StateFn::new(lex_inside_mutation))
        }
        w => l.errorf(format!("Invalid operation type: {w}")),
    }
}

/// Lexes and absorbs the text inside a block (covered by braces).
fn lex_block_content(l: &mut Lexer<'_>) -> Option<StateFn> {
    lex_content(l, LEFT_CURL, RIGHT_CURL, StateFn::new(lex_upsert_block))
}

/// Lexes the whole of the @if directive in a mutation block (covered by small brackets).
fn lex_if_content(l: &mut Lexer<'_>) -> Option<StateFn> {
    let r = l.next();
    if r != Some(AT) {
        return l.errorf(format!("Expected [@], found; [{}]", unicode_hash(r)));
    }
    l.accept_run(is_name_suffix);
    let w = word(l);
    if w != "@if" {
        return l.errorf(format!("Expected @if, found [{w}]"));
    }
    lex_content(l, '(', ')', StateFn::new(lex_inside_mutation))
}

fn lex_content(
    l: &mut Lexer<'_>,
    left_rune: char,
    right_rune: char,
    return_to: StateFn,
) -> Option<StateFn> {
    let mut depth = 0;
    loop {
        match l.next() {
            None => return l.errorf("Matching brackets not found"),
            Some(QUOTE) => {
                if let Err(e) = l.lex_quoted_string() {
                    return l.errorf(e);
                }
            }
            Some(r) if r == left_rune => depth += 1,
            Some(r) if r == right_rune => {
                depth -= 1;
                if depth < 0 {
                    return l.errorf(format!("Unopened {right_rune} found"));
                }
                if depth == 0 {
                    l.emit(ITEM_UPSERT_BLOCK_OP_CONTENT);
                    return Some(return_to);
                }
            }
            Some(_) => {}
        }
    }
}

fn lex_inside_mutation(l: &mut Lexer<'_>) -> Option<StateFn> {
    l.set_mode(StateFn::new(lex_inside_mutation));
    loop {
        match l.next() {
            Some(AT) => {
                l.backup();
                return Some(StateFn::new(lex_if_content));
            }
            Some(RIGHT_CURL) => {
                l.set_depth(l.depth() - 1);
                l.emit(ITEM_RIGHT_CURL);
                if l.depth() == 0 {
                    return Some(LEX_TOP_LEVEL);
                }
            }
            Some(LEFT_CURL) => {
                l.set_depth(l.depth() + 1);
                l.emit(ITEM_LEFT_CURL);
                if l.depth() >= 2 {
                    return Some(StateFn::new(lex_text_mutation));
                }
            }
            Some(r) if is_space_or_eol(r) => l.ignore(),
            Some(r) if is_name_begin(r) => return Some(StateFn::new(lex_name_mutation)),
            Some('#') => return Some(StateFn::new(lex_comment)),
            None => return l.errorf("Unclosed mutation action"),
            r => {
                return l.errorf(format!(
                    "Unrecognized character inside mutation: {}",
                    unicode_hash(r)
                ));
            }
        }
    }
}

fn lex_inside_schema(l: &mut Lexer<'_>) -> Option<StateFn> {
    l.set_mode(StateFn::new(lex_inside_schema));
    loop {
        match l.next() {
            Some(RIGHT_ROUND) => l.emit(ITEM_RIGHT_ROUND),
            Some(LEFT_ROUND) => l.emit(ITEM_LEFT_ROUND),
            Some(RIGHT_CURL) => {
                l.set_depth(l.depth() - 1);
                l.emit(ITEM_RIGHT_CURL);
                if l.depth() == 0 {
                    return Some(LEX_TOP_LEVEL);
                }
            }
            Some(LEFT_CURL) => {
                l.set_depth(l.depth() + 1);
                l.emit(ITEM_LEFT_CURL);
            }
            Some(LEFT_SQUARE) => l.emit(ITEM_LEFT_SQUARE),
            Some(RIGHT_SQUARE) => l.emit(ITEM_RIGHT_SQUARE),
            Some(r) if is_space_or_eol(r) => l.ignore(),
            Some(LS_THAN) => return Some(StateFn::new(lex_iri_ref)),
            Some(r) if is_name_begin(r) => return Some(StateFn::new(lex_arg_name)),
            Some('#') => return Some(StateFn::new(lex_comment)),
            Some(COLON) => l.emit(ITEM_COLON),
            Some(COMMA) => l.emit(ITEM_COMMA),
            None => return l.errorf("Unclosed schema action"),
            r => {
                return l.errorf(format!(
                    "Unrecognized character inside schema: {}",
                    unicode_hash(r)
                ));
            }
        }
    }
}

fn lex_func_or_arg(l: &mut Lexer<'_>) -> Option<StateFn> {
    l.set_mode(StateFn::new(lex_func_or_arg));
    let mut empty = false;
    loop {
        let Some(r) = l.next() else {
            return l.errorf("Unclosed Brackets");
        };
        if r == AT {
            l.emit(ITEM_AT);
            return Some(StateFn::new(lex_directive_or_lang_list));
        } else if is_name_begin(r) || is_number(r) {
            return Some(StateFn::new(lex_arg_name));
        } else if r == SLASH && empty {
            // if argument starts with '/' it's a regex, otherwise it's a division
            return lex_regex(l);
        } else if is_math_op(r) {
            l.emit(ITEM_MATH_OP);
        } else if is_inequality_op(r) {
            if r == EQUAL && !l.peek().is_some_and(is_inequality_op) {
                l.emit(ITEM_EQUAL);
                continue;
            }
            if r == LS_THAN {
                let p = l.peek();
                if !p.is_some_and(is_space) && p != Some('=') {
                    // as long as its not '=' or ' '
                    return Some(StateFn::new(lex_iri_ref));
                }
            }
            if l.peek().is_some_and(is_inequality_op) {
                l.next();
            }
            l.emit(ITEM_MATH_OP);
        } else if r == LEFT_ROUND {
            l.emit(ITEM_LEFT_ROUND);
            l.set_arg_depth(l.arg_depth() + 1);
        } else if r == RIGHT_ROUND {
            if l.arg_depth() == 0 {
                return l.errorf("Unexpected right round bracket");
            }
            l.set_arg_depth(l.arg_depth() - 1);
            l.emit(ITEM_RIGHT_ROUND);
            if empty {
                return l.errorf("Empty Argument");
            }
            if l.arg_depth() == 0 {
                return Some(LEX_QUERY); // Filter directive is done.
            }
        } else if is_space_or_eol(r) {
            l.ignore();
        } else if r == COMMA {
            if empty {
                return l.errorf("Consecutive commas not allowed.");
            }
            empty = true;
            l.emit(ITEM_COMMA);
        } else if is_dollar(r) {
            l.emit(ITEM_DOLLAR);
        } else if r == COLON {
            l.emit(ITEM_COLON);
        } else if r == QUOTE {
            empty = false;
            if let Err(e) = l.lex_quoted_string() {
                return l.errorf(e);
            }
            l.emit(ITEM_NAME);
        } else if is_end_literal(r) {
            empty = false;
            l.accept_until(is_end_literal); // This call will backup the ending ".
            l.next(); // Consume the " .
            l.emit(ITEM_NAME);
        } else if r == LEFT_SQUARE {
            l.emit(ITEM_LEFT_SQUARE);
        } else if r == RIGHT_SQUARE {
            l.emit(ITEM_RIGHT_SQUARE);
        } else if r == LEFT_CURL {
            empty = false;
            // Braces are emitted without affecting the argument depth; the parser decides whether they are legal
            // where they stand.
            l.emit(ITEM_LEFT_CURL);
        } else if r == RIGHT_CURL {
            l.emit(ITEM_RIGHT_CURL);
        } else if r == '#' {
            return Some(StateFn::new(lex_comment));
        } else if r == '.' {
            l.emit(ITEM_PERIOD);
        } else {
            return l.errorf(format!(
                "Unrecognized character inside a func: {}",
                unicode_hash(Some(r))
            ));
        }
    }
}

fn lex_top_level(l: &mut Lexer<'_>) -> Option<StateFn> {
    // BlockDepth tells the nesting of blocks. Currently, only the upsert block has nested mutation, query and
    // fragment blocks.
    if l.block_depth() != 0 {
        return Some(StateFn::new(lex_upsert_block));
    }
    l.set_mode(LEX_TOP_LEVEL);
    loop {
        match l.next() {
            Some(LEFT_CURL) => {
                l.set_depth(l.depth() + 1); // one level down.
                l.emit(ITEM_LEFT_CURL);
                return Some(StateFn::new(lex_identify_mutation_or_query));
            }
            Some(RIGHT_CURL) => return l.errorf("Too many right curl"),
            None => break,
            Some('#') => return Some(StateFn::new(lex_comment)),
            Some(LEFT_ROUND) => {
                l.backup();
                l.emit(ITEM_TEXT);
                l.next();
                l.emit(ITEM_LEFT_ROUND);
                l.set_arg_depth(l.arg_depth() + 1);
                return Some(LEX_QUERY);
            }
            Some(r) if is_space_or_eol(r) => l.ignore(),
            Some(r) if is_name_begin(r) => {
                l.backup();
                return Some(StateFn::new(lex_operation_type));
            }
            Some(_) => {}
        }
    }
    if l.pos() > l.start() {
        l.emit(ITEM_TEXT);
    }
    l.emit(ITEM_EOF);
    None
}

fn lex_identify_mutation_or_query(l: &mut Lexer<'_>) -> Option<StateFn> {
    l.set_mode(StateFn::new(lex_identify_mutation_or_query));
    loop {
        match l.next() {
            Some(r) if is_space_or_eol(r) => l.ignore(),
            Some(r) if is_name_begin(r) => {
                l.accept_run(is_name_suffix);
                let op = word(l);
                // If it is a mutation
                if op == "set" || op == "delete" || op == "del" {
                    l.emit(ITEM_MUTATION_OP);
                    return Some(StateFn::new(lex_inside_mutation));
                }
                // else it is a query
                l.emit(ITEM_NAME);
                return Some(LEX_QUERY);
            }
            Some(LS_THAN) => {
                l.backup();
                return Some(LEX_QUERY);
            }
            Some('#') => return Some(StateFn::new(lex_comment)),
            None => return l.errorf("Invalid DQL"),
            r => {
                return l.errorf(format!(
                    "Unexpected character while lexing DQL: {}",
                    unicode_hash(r)
                ));
            }
        }
    }
}

/// Lexes the input inside a query block.
fn lex_query(l: &mut Lexer<'_>) -> Option<StateFn> {
    l.set_mode(LEX_QUERY);
    loop {
        match l.next() {
            Some(PERIOD) => l.emit(ITEM_PERIOD),
            Some(RIGHT_CURL) => {
                l.set_depth(l.depth() - 1);
                l.emit(ITEM_RIGHT_CURL);
                if l.depth() == 0 {
                    return Some(LEX_TOP_LEVEL);
                }
            }
            Some(LEFT_CURL) => {
                l.set_depth(l.depth() + 1);
                l.emit(ITEM_LEFT_CURL);
            }
            None => return l.errorf("Unclosed action"),
            Some(r) if is_space_or_eol(r) => l.ignore(),
            Some(COMMA) => l.emit(ITEM_COMMA),
            Some(r) if is_name_begin(r) => return Some(StateFn::new(lex_name)),
            Some('#') => return Some(StateFn::new(lex_comment)),
            Some('-') => l.emit(ITEM_MATH_OP),
            Some(LEFT_ROUND) => {
                l.emit(ITEM_LEFT_ROUND);
                l.accept_run(is_space);
                l.ignore();
                l.set_arg_depth(l.arg_depth() + 1);
                return Some(StateFn::new(lex_func_or_arg));
            }
            Some(COLON) => l.emit(ITEM_COLON),
            Some(AT) => {
                l.emit(ITEM_AT);
                return Some(StateFn::new(lex_directive_or_lang_list));
            }
            Some(LS_THAN) => return Some(StateFn::new(lex_iri_ref)),
            Some(STAR) => l.emit(ITEM_STAR),
            r => {
                return l.errorf(format!(
                    "Unrecognized character in lexText: {}",
                    unicode_hash(r)
                ));
            }
        }
    }
}

fn lex_iri_ref(l: &mut Lexer<'_>) -> Option<StateFn> {
    if let Err(e) = iri_ref(l, ITEM_NAME) {
        return l.errorf(e);
    }
    l.mode()
}

/// Lexes a directive or a language list, right after an `@`.
fn lex_directive_or_lang_list(l: &mut Lexer<'_>) -> Option<StateFn> {
    let r = l.next();
    // Check first character.
    if !r.is_some_and(|r| is_name_begin(r) || r == PERIOD || r == STAR) {
        return l.errorf(format!(
            "Unrecognized character in lexDirective: {}",
            unicode_hash(r)
        ));
    }
    l.backup();
    loop {
        let r = l.next();
        if r == Some(PERIOD) {
            l.emit(ITEM_NAME);
            return l.mode();
        }
        if r.is_some_and(is_lang_or_directive) {
            continue;
        }
        l.backup();
        l.emit(ITEM_NAME);
        break;
    }
    l.mode()
}

fn lex_name(l: &mut Lexer<'_>) -> Option<StateFn> {
    l.accept_run(is_name_suffix);
    l.emit(ITEM_NAME);
    l.mode()
}

/// Lexes a comment.
fn lex_comment(l: &mut Lexer<'_>) -> Option<StateFn> {
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

/// Lexes the mutation operation, which could be set or delete.
fn lex_name_mutation(l: &mut Lexer<'_>) -> Option<StateFn> {
    // The caller already checked isNameBegin, and absorbed one rune.
    while l.next().is_some_and(is_name_suffix) {}
    l.backup();
    l.emit(ITEM_MUTATION_OP);
    l.mode()
}

/// Lexes and absorbs the text inside a mutation operation block.
fn lex_text_mutation(l: &mut Lexer<'_>) -> Option<StateFn> {
    loop {
        match l.next() {
            None => return l.errorf("Unclosed mutation text"),
            Some(QUOTE) => return Some(StateFn::new(lex_mutation_value)),
            Some(LEFT_CURL) => return l.errorf("Invalid character '{' inside mutation text"),
            // Absorb everything until we find '}'.
            Some(r) if r != RIGHT_CURL => {}
            Some(_) => {
                l.backup();
                l.emit(ITEM_MUTATION_OP_CONTENT);
                break;
            }
        }
    }
    Some(StateFn::new(lex_inside_mutation))
}

/// Absorbs the object value.
fn lex_mutation_value(l: &mut Lexer<'_>) -> Option<StateFn> {
    loop {
        match l.next() {
            None => return l.errorf("Unclosed mutation value"),
            Some(QUOTE) => break,
            Some('\\') => {
                l.next(); // skip one.
            }
            Some(_) => {}
        }
    }
    Some(StateFn::new(lex_text_mutation))
}

fn lex_regex(l: &mut Lexer<'_>) -> Option<StateFn> {
    loop {
        match l.next() {
            None => return l.errorf("Unclosed regexp"),
            Some('\\') => {
                l.next();
            }
            Some('/') => break,
            Some(_) => {}
        }
    }
    l.accept_run(is_regex_flag);
    l.emit(ITEM_REGEX);
    l.mode()
}

/// Lexes a query, mutation or schema operation type.
fn lex_operation_type(l: &mut Lexer<'_>) -> Option<StateFn> {
    l.accept_run(is_name_suffix);
    match word(l) {
        "mutation" => {
            l.emit(ITEM_OP_TYPE);
            Some(StateFn::new(lex_inside_mutation))
        }
        "fragment" | "query" => {
            l.emit(ITEM_OP_TYPE);
            Some(LEX_QUERY)
        }
        "schema" => {
            l.emit(ITEM_OP_TYPE);
            Some(StateFn::new(lex_inside_schema))
        }
        "upsert" => {
            l.emit(ITEM_UPSERT_BLOCK);
            Some(StateFn::new(lex_upsert_block))
        }
        w => l.errorf(format!("Invalid operation type: {w}")),
    }
}

/// Lexes and emits the name part of an argument.
fn lex_arg_name(l: &mut Lexer<'_>) -> Option<StateFn> {
    l.accept_run(is_name_suffix);
    l.emit(ITEM_NAME);
    l.mode()
}

/// Whether the rune is a dollar.
fn is_dollar(r: char) -> bool {
    r == '$'
}

/// Whether the rune is a tab or space.
pub(crate) fn is_space(r: char) -> bool {
    r == '\u{0009}' || r == '\u{0020}'
}

/// Whether the rune is a quotation mark or ends a line.
fn is_end_literal(r: char) -> bool {
    r == '"' || r == '\u{000d}' || r == '\u{000a}'
}

fn is_lang_or_directive(r: char) -> bool {
    is_name_begin(r) || r == '-' || r.is_ascii_digit() || r == '*'
}

/// Whether the rune is a letter, `_` or `~`.
fn is_name_begin(r: char) -> bool {
    r.is_ascii_alphabetic() || r == '_' || r == '~'
}

fn is_math_op(r: char) -> bool {
    matches!(r, '+' | '-' | '*' | '/' | '%')
}

fn is_inequality_op(r: char) -> bool {
    matches!(r, '<' | '>' | '=' | '!')
}

fn is_number(r: char) -> bool {
    r.is_ascii_digit()
}

fn is_name_suffix(r: char) -> bool {
    if is_math_op(r) {
        return false;
    }
    is_name_begin(r) || is_number(r) || r == '.'
}

fn is_regex_flag(r: char) -> bool {
    r.is_ascii_alphabetic()
}
