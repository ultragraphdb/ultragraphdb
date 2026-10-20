/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go and lex/iri.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use core::fmt::{Display, Formatter};

use super::{LexError, LexErrorEnum};
use crate::utils::go_quote::quote_rune;

impl Display for LexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match &self.0 {
            LexErrorEnum::Lexing(text) => f.write_str(text),
            LexErrorEnum::Located {
                line,
                column,
                message,
            } => write!(f, "line {line} column {column}: {message}"),
            LexErrorEnum::PeekOutOfRange => f.write_str("Out of range for peek"),
            LexErrorEnum::NoOpeningQuote => f.write_str("String should start with quote."),
            LexErrorEnum::UnexpectedEndOfInput => f.write_str("Unexpected end of input."),
            // Go formats the EOF rune (-1) with %c as the replacement character.
            LexErrorEnum::InvalidEscapeChar(r) => {
                write!(
                    f,
                    "Not a valid escape char: '{}'",
                    r.unwrap_or(char::REPLACEMENT_CHARACTER)
                )
            }
            LexErrorEnum::UnexpectedEndOfIri => f.write_str("Unexpected end of IRI"),
            LexErrorEnum::UnexpectedIriChar(r) => write!(
                f,
                "Unexpected character {} while parsing IRI",
                quote_rune(*r)
            ),
        }
    }
}
