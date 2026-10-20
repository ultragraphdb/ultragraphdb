/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Whether the rune is a line feed or a carriage return.
pub fn is_end_of_line(r: char) -> bool {
    r == '\n' || r == '\r'
}

/// Whether the rune just read after a backslash is an escape character, `ECHAR ::= '\' [uvtbnrf"'\]`; the end of
/// the input (`None`) is not. Go's `Lexer.IsEscChar`, which reads nothing from the lexer.
pub fn is_esc_char(r: Option<char>) -> bool {
    matches!(
        r,
        Some('u' | 'v' | 't' | 'b' | 'n' | 'r' | 'f' | '"' | '\'' | '\\')
    )
}
