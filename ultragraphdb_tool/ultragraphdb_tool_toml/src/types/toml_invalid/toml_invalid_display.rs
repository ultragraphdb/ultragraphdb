/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::toml_invalid::TomlInvalid;

impl fmt::Display for TomlInvalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtf8 => f.write_str("the line is not UTF-8"),
            Self::ControlCharacter => f.write_str("a control character other than tab"),
            Self::MalformedKey => f.write_str("a key that is empty or not bare"),
            Self::MalformedHeader => f.write_str("a malformed table header"),
            Self::KeyWithoutValue => f.write_str("a key without a value"),
            Self::MalformedValue => f.write_str("a value of no TOML kind"),
            Self::UnterminatedString => f.write_str("a string not closed on its line"),
            Self::InvalidEscape => f.write_str("an invalid escape"),
            Self::MalformedInteger => f.write_str("a malformed integer"),
            Self::LeadingZero => f.write_str("an integer with a leading zero"),
            Self::IntegerOutOfRange => f.write_str("an integer outside the 64-bit signed range"),
            Self::TrailingCharacters => {
                f.write_str("characters other than a comment after the value or header")
            }
            Self::DuplicateKey(key) => write!(f, "the key `{key}` defined twice"),
            Self::DuplicateTable(name) => write!(f, "the table `[{name}]` defined twice"),
        }
    }
}
