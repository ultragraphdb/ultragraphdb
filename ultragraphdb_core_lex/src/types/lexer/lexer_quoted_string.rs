/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::Lexer;
use crate::errors::lex_error::{LexError, LexErrorEnum};
use crate::utils::lexer::is_esc_char;

const QUOTE: char = '"';

impl Lexer<'_> {
    /// Lexes a quoted string whose opening `"` the caller has read, through its closing `"`, skipping escaped
    /// characters. The string stays in `start..pos`.
    ///
    /// # Errors
    ///
    /// [`LexErrorEnum::NoOpeningQuote`] when the rune last read is not `"`,
    /// [`LexErrorEnum::UnexpectedEndOfInput`] when the string is not closed, [`LexErrorEnum::InvalidEscapeChar`]
    /// when a backslash escapes no escape character.
    pub fn lex_quoted_string(&mut self) -> Result<(), LexError> {
        self.backup();
        if self.next() != Some(QUOTE) {
            return Err(LexError::new(LexErrorEnum::NoOpeningQuote));
        }
        loop {
            match self.next() {
                None => return Err(LexError::new(LexErrorEnum::UnexpectedEndOfInput)),
                Some('\\') => {
                    let r = self.next();
                    if !is_esc_char(r) {
                        return Err(LexError::new(LexErrorEnum::InvalidEscapeChar(r)));
                    }
                    // eat the next char
                }
                Some(QUOTE) => return Ok(()),
                Some(_) => {}
            }
        }
    }
}
