/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod toml_invalid_display;

/// A rule of TOML 1.0 a document breaks, so that no TOML parser reads it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TomlInvalid {
    /// The bytes of the line are not UTF-8.
    InvalidUtf8,
    /// A control character other than tab outside a string's escapes: in a string, a comment, or between tokens; a
    /// carriage return is one unless a line feed follows it.
    ControlCharacter,
    /// A key that is empty or holds a character outside `A–Z`, `a–z`, `0–9`, `_` and `-`.
    MalformedKey,
    /// A table header that is not closed, empty, or holds a character outside a bare key's.
    MalformedHeader,
    /// A key followed by no `=`, or by `=` and no value.
    KeyWithoutValue,
    /// A value of no kind TOML knows, as `a = yes`.
    MalformedValue,
    /// A string not closed on its line.
    UnterminatedString,
    /// An escape TOML does not define, a `\u` or `\U` without its hexadecimal digits, or one naming a surrogate or a
    /// code point past U+10FFFF.
    InvalidEscape,
    /// An integer with a character other than digits and `_`, a sign without digits, or a `_` not between digits.
    MalformedInteger,
    /// An integer of more than one digit starting with `0`.
    LeadingZero,
    /// An integer outside the range of a 64-bit signed integer.
    IntegerOutOfRange,
    /// Something other than whitespace and a comment after a value or a table header.
    TrailingCharacters,
    /// The key, named here, is defined twice in one table, or names a table defined after it in the root table.
    DuplicateKey(String),
    /// The table, named here, has two headers.
    DuplicateTable(String),
}
