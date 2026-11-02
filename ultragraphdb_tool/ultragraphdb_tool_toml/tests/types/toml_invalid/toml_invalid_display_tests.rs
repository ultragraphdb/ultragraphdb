/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_toml::TomlInvalid;

#[test]
fn test_each_rule_is_named() {
    let names = [
        (TomlInvalid::InvalidUtf8, "the line is not UTF-8"),
        (
            TomlInvalid::ControlCharacter,
            "a control character other than tab",
        ),
        (TomlInvalid::MalformedKey, "a key that is empty or not bare"),
        (TomlInvalid::MalformedHeader, "a malformed table header"),
        (TomlInvalid::KeyWithoutValue, "a key without a value"),
        (TomlInvalid::MalformedValue, "a value of no TOML kind"),
        (
            TomlInvalid::UnterminatedString,
            "a string not closed on its line",
        ),
        (TomlInvalid::InvalidEscape, "an invalid escape"),
        (TomlInvalid::MalformedInteger, "a malformed integer"),
        (TomlInvalid::LeadingZero, "an integer with a leading zero"),
        (
            TomlInvalid::IntegerOutOfRange,
            "an integer outside the 64-bit signed range",
        ),
        (
            TomlInvalid::TrailingCharacters,
            "characters other than a comment after the value or header",
        ),
        (
            TomlInvalid::DuplicateKey("sync".to_string()),
            "the key `sync` defined twice",
        ),
        (
            TomlInvalid::DuplicateTable("storage".to_string()),
            "the table `[storage]` defined twice",
        ),
    ];
    for (reason, name) in names {
        assert_eq!(reason.to_string(), name);
    }
}
