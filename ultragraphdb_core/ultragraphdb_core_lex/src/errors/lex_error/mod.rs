/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go and lex/iri.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod lex_error_display;
mod lex_error_error;

/// Why lexing, or reading the lexed items, failed. Its `Display` is Dgraph's error text.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LexError(pub LexErrorEnum);

/// Detailed classification of lexer errors.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LexErrorEnum {
    /// The lexer emitted an error item; the item's text, `while lexing <input> at line L column C: <message>`.
    Lexing(String),
    /// An error a parser raised at an item's position, `line L column C: <message>`.
    Located {
        line: i32,
        column: i32,
        message: String,
    },
    /// `ItemIterator::peek` asked for more items than remain.
    PeekOutOfRange,
    /// A quoted string did not start with `"`.
    NoOpeningQuote,
    /// A quoted string was not closed before the end of the input.
    UnexpectedEndOfInput,
    /// A backslash in a quoted string was followed by this rune, or by the end of the input (`None`), which is not
    /// an escape character.
    InvalidEscapeChar(Option<char>),
    /// An IRI was not closed with `>` before the end of the input.
    UnexpectedEndOfIri,
    /// An IRI was ended by this rune instead of `>`.
    UnexpectedIriChar(char),
}

impl LexError {
    pub(crate) fn new(variant: LexErrorEnum) -> Self {
        Self(variant)
    }

    #[allow(non_snake_case)]
    pub fn Lexing(msg: String) -> Self {
        Self(LexErrorEnum::Lexing(msg))
    }

    #[allow(non_snake_case)]
    pub fn Located(line: i32, column: i32, message: String) -> Self {
        Self(LexErrorEnum::Located {
            line,
            column,
            message,
        })
    }

    #[allow(non_snake_case)]
    pub fn PeekOutOfRange() -> Self {
        Self(LexErrorEnum::PeekOutOfRange)
    }

    #[allow(non_snake_case)]
    pub fn NoOpeningQuote() -> Self {
        Self(LexErrorEnum::NoOpeningQuote)
    }

    #[allow(non_snake_case)]
    pub fn UnexpectedEndOfInput() -> Self {
        Self(LexErrorEnum::UnexpectedEndOfInput)
    }

    #[allow(non_snake_case)]
    pub fn InvalidEscapeChar(value: Option<char>) -> Self {
        Self(LexErrorEnum::InvalidEscapeChar(value))
    }

    #[allow(non_snake_case)]
    pub fn UnexpectedEndOfIri() -> Self {
        Self(LexErrorEnum::UnexpectedEndOfIri)
    }

    #[allow(non_snake_case)]
    pub fn UnexpectedIriChar(value: char) -> Self {
        Self(LexErrorEnum::UnexpectedIriChar(value))
    }
}
