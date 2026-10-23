/*
 * Copyright 2020 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/internal/jsonwire/wire.go (InvalidTextError), jsontext/errors.go
 * (wrapSyntacticError under ReportErrorsWithLegacySemantics) and v2_scanner.go (transformSyntacticError).
 * Changed for ultragraphdb: translated to Rust; the error before its offset is one enum.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_error_json::JsonError;

use crate::utils::{go_strconv, go_utf8};

/// Why the scan of a JSON value stopped, before the decoder places it at an offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScanError {
    /// Go's `io.ErrUnexpectedEOF`: the input ended inside a value.
    UnexpectedEof,
    /// Go's `jsonwire.InvalidTextError`: `what` is the raw invalid text, `label` names it and `where_` says where
    /// it stood.
    InvalidText {
        label: &'static str,
        what: Vec<u8>,
        where_: String,
    },
    /// Go's `errMaxDepth`, an object or array nested deeper than 10000.
    MaxDepth,
    /// The reader failed, with its error's text.
    Read(String),
}

/// Go's `jsonwire.QuoteRune`: the first rune in single quotes, a byte that is not UTF-8 as `'\xff'`.
fn quote_rune(b: &[u8]) -> String {
    match go_utf8::decode_rune(b) {
        (go_utf8::RUNE_ERROR, 1) => format!("'\\x{:x}'", b[0]),
        (r, _) => go_strconv::quote_rune(char::from_u32(r).unwrap_or(char::REPLACEMENT_CHARACTER)),
    }
}

impl ScanError {
    /// Go's `NewInvalidCharacterError`: the first rune of `prefix`, invalid at `where_`.
    pub(crate) fn invalid_character(prefix: &[u8], where_: impl Into<String>) -> Self {
        let (_, n) = go_utf8::decode_rune(prefix);
        ScanError::InvalidText {
            label: "character",
            what: prefix[..n].to_vec(),
            where_: where_.into(),
        }
    }

    /// Go's `NewInvalidEscapeSequenceError` for the escapes Go 1.27's v1 decoder checks: no surrogate pairs.
    pub(crate) fn invalid_escape(what: &[u8]) -> Self {
        ScanError::InvalidText {
            label: "escape sequence",
            what: what.to_vec(),
            where_: "in string".to_string(),
        }
    }

    /// Go's `InvalidTextError.Error`: one rune quoted as a rune, text that needs escaping quoted as a string, other
    /// text in backquotes.
    fn invalid_text(label: &str, what: &[u8], where_: &str) -> String {
        let runes: Vec<(u32, usize)> = {
            let mut out = Vec::new();
            let mut i = 0;
            while i < what.len() {
                let (r, n) = go_utf8::decode_rune(&what[i..]);
                out.push((r, n));
                i += n;
            }
            out
        };
        let need_escape = runes.iter().any(|&(r, _)| {
            let c = char::from_u32(r).unwrap_or(char::REPLACEMENT_CHARACTER);
            r == u32::from('`')
                || r == go_utf8::RUNE_ERROR
                || c.is_whitespace()
                || !go_strconv::is_print(c)
        });
        let what = match std::str::from_utf8(what) {
            _ if runes.len() == 1 => quote_rune(what),
            Ok(s) if !need_escape => format!("`{s}`"),
            _ => go_strconv::quote(what),
        };
        let text = format!("invalid {label} {what} {where_}");
        text.strip_suffix(' ').map(str::to_string).unwrap_or(text)
    }

    /// The error at `offset` as Go's v1 API reports it: `wrapSyntacticError` with legacy semantics, then
    /// `transformSyntacticError`. Invalid text moves the offset past it and a structural error past one byte; the
    /// input's end is `unexpected end of JSON input`; v2's wording becomes v1's.
    pub(crate) fn at(self, offset: usize) -> JsonError {
        let offset = offset as i64;
        match self {
            ScanError::UnexpectedEof => {
                JsonError::Syntax("unexpected end of JSON input".to_string(), offset)
            }
            ScanError::MaxDepth => JsonError::Syntax("exceeded max depth".to_string(), offset + 1),
            ScanError::Read(msg) => JsonError::Read(msg),
            ScanError::InvalidText {
                label,
                what,
                where_,
            } => {
                let mut msg = Self::invalid_text(label, &what, &where_);
                if let Some(i) = msg.find(" (expecting")
                    && !msg.contains(" in literal")
                {
                    msg.truncate(i);
                }
                let msg = [
                    ("object name", "object key"),
                    ("at start of value", "looking for beginning of value"),
                    (
                        "at start of string",
                        "looking for beginning of object key string",
                    ),
                    ("after object value", "after object key:value pair"),
                    ("in number", "in numeric literal"),
                ]
                .iter()
                .fold(msg, |m, (from, to)| m.replace(from, to));
                JsonError::Syntax(msg, offset + what.len() as i64)
            }
        }
    }
}
