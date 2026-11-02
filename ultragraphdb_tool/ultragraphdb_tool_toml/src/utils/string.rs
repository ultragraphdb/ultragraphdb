/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Basic strings: one line between double quotes, with TOML's escapes.

use std::str::Chars;

use crate::errors::toml_error::TomlError;
use crate::types::toml_invalid::TomlInvalid;
use crate::utils::chars::is_forbidden_control;

/// The basic string whose opening quote precedes `rest`, its escapes resolved, and the rest of the line after its
/// closing quote.
///
/// # Errors
///
/// `UnterminatedString` when the line ends first, `InvalidEscape` for an escape TOML does not define,
/// `ControlCharacter` for an unescaped control character other than tab.
pub fn basic_string(rest: &str, line: usize) -> Result<(String, &str), TomlError> {
    let mut text = String::new();
    let mut chars = rest.chars();
    // Bounded by the line: every turn takes at least one character from it.
    while let Some(c) = chars.next() {
        match c {
            '"' => return Ok((text, chars.as_str())),
            '\\' => text.push(escape(&mut chars, line)?),
            c if is_forbidden_control(c) => {
                return Err(TomlError::invalid(TomlInvalid::ControlCharacter, line));
            }
            c => text.push(c),
        }
    }
    Err(TomlError::invalid(TomlInvalid::UnterminatedString, line))
}

/// The character the escape after a backslash names, taken from `chars`.
fn escape(chars: &mut Chars<'_>, line: usize) -> Result<char, TomlError> {
    match chars.next() {
        Some('b') => Ok('\u{8}'),
        Some('t') => Ok('\t'),
        Some('n') => Ok('\n'),
        Some('f') => Ok('\u{c}'),
        Some('r') => Ok('\r'),
        Some('"') => Ok('"'),
        Some('\\') => Ok('\\'),
        Some('u') => unicode(chars, 4, line),
        Some('U') => unicode(chars, 8, line),
        Some(_) | None => Err(TomlError::invalid(TomlInvalid::InvalidEscape, line)),
    }
}

/// The scalar value `digits` hexadecimal digits from `chars` name: a code point that is no surrogate and at most
/// U+10FFFF.
fn unicode(chars: &mut Chars<'_>, digits: usize, line: usize) -> Result<char, TomlError> {
    debug_assert!(digits == 4 || digits == 8);
    let refused = || TomlError::invalid(TomlInvalid::InvalidEscape, line);
    let mut code: u32 = 0;
    for _ in 0..digits {
        let digit = chars
            .next()
            .and_then(|c| c.to_digit(16))
            .ok_or_else(refused)?;
        // Eight digits at most, so the shift never pushes a set bit out of the u32.
        code = (code << 4) | digit;
    }
    char::from_u32(code).ok_or_else(refused)
}
