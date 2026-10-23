/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/iri.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::lex_error::{LexError, LexErrorEnum};
use crate::types::item_type::ItemType;
use crate::types::lexer::Lexer;

/// Lexes an IRIREF whose `<` the caller has read: emits its content as `styp`, without the brackets, and ignores
/// the closing `>`.
///
/// # Errors
///
/// [`LexErrorEnum::UnexpectedEndOfIri`] when the input ends before `>`, [`LexErrorEnum::UnexpectedIriChar`] when
/// a rune not allowed in an IRI does.
pub fn iri_ref(l: &mut Lexer<'_>, styp: ItemType) -> Result<(), LexError> {
    l.ignore(); // ignore '<'
    l.accept_run_rec(is_iri_ref_char);
    l.emit(styp); // will emit without '<' and '>'
    match l.next() {
        None => Err(LexError::new(LexErrorEnum::UnexpectedEndOfIri)),
        Some('>') => {
            l.ignore(); // ignore '>'
            Ok(())
        }
        Some(r) => Err(LexError::new(LexErrorEnum::UnexpectedIriChar(r))),
    }
}

/// Whether the rune is allowed in an IRIREF, `IRIREF ::= '<' ([^#x00-#x20<>"{}|^`\] | UCHAR)* '>'`. A backslash
/// reads ahead for a `UCHAR`.
pub fn is_iri_ref_char(r: char, l: &mut Lexer<'_>) -> bool {
    if r <= ' ' {
        // no chars b/w 0x00 to 0x20 inclusive
        return false;
    }
    match r {
        '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' => false,
        '\\' => {
            let r2 = l.next();
            if r2 != Some('u') && r2 != Some('U') {
                l.backup();
                return false;
            }
            has_u_chars(r2, l)
        }
        _ => true,
    }
}

/// Whether the lexer is at an escaped Unicode character whose `u` or `U` is `r`,
/// `UCHAR ::= '\u' HEX HEX HEX HEX | '\U' HEX HEX HEX HEX HEX HEX HEX HEX`.
pub fn has_u_chars(r: Option<char>, l: &mut Lexer<'_>) -> bool {
    let times = match r {
        Some('u') => 4,
        Some('U') => 8,
        _ => return false,
    };
    times == l.accept_run_times(is_hex, times)
}

/// Whether the lexer is at an escaped hexadecimal byte whose `x` is `r` (e.g. `\xFE`), `XCHAR ::= '\x' HEX HEX`.
pub fn has_x_chars(r: Option<char>, l: &mut Lexer<'_>) -> bool {
    if r != Some('x') {
        return false;
    }
    let times = 2;
    times == l.accept_run_times(is_hex, times)
}

/// `HEX ::= [0-9] | [A-F] | [a-f]`.
pub fn is_hex(r: char) -> bool {
    r.is_ascii_hexdigit()
}
