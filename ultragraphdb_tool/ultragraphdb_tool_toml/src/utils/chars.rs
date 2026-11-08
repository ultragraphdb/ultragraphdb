/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The classes of characters TOML 1.0 names, and the end of a line.

use crate::errors::toml_error::TomlError;
use crate::types::toml_invalid::TomlInvalid;

/// A character of a bare key: `A–Z`, `a–z`, `0–9`, `_` and `-`.
pub fn is_bare(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// A control character TOML refuses outside an escape: U+0000 to U+0008, U+000A to U+001F and U+007F; tab is allowed.
pub fn is_forbidden_control(c: char) -> bool {
    (c < ' ' && c != '\t') || c == '\u{7f}'
}

/// `rest` without its leading spaces and tabs, TOML's whitespace.
pub fn skip_whitespace(rest: &str) -> &str {
    rest.trim_start_matches([' ', '\t'])
}

/// The run of characters `is_in` takes at the start of `rest`, and what follows it. Each half is computed from
/// `rest` alone, so neither indexes nor splits it.
pub fn split_run(rest: &str, is_in: impl Fn(char) -> bool) -> (&str, &str) {
    let run = rest.split(|c: char| !is_in(c)).next().unwrap_or_default();
    (run, rest.trim_start_matches(is_in))
}

/// A comment, `#` to the end of the line, holds no forbidden control character.
///
/// # Errors
///
/// `ControlCharacter` on line `line`.
pub fn comment(rest: &str, line: usize) -> Result<(), TomlError> {
    if rest.chars().any(is_forbidden_control) {
        Err(TomlError::invalid(TomlInvalid::ControlCharacter, line))
    } else {
        Ok(())
    }
}

/// What follows a value or a header: whitespace, then nothing or a comment.
///
/// # Errors
///
/// `ControlCharacter` for a control character there or in the comment, `TrailingCharacters` for anything else.
pub fn end_of_line(rest: &str, line: usize) -> Result<(), TomlError> {
    let rest = skip_whitespace(rest);
    match rest.chars().next() {
        None => Ok(()),
        Some('#') => comment(rest, line),
        Some(c) if is_forbidden_control(c) => {
            Err(TomlError::invalid(TomlInvalid::ControlCharacter, line))
        }
        Some(_) => Err(TomlError::invalid(TomlInvalid::TrailingCharacters, line)),
    }
}
