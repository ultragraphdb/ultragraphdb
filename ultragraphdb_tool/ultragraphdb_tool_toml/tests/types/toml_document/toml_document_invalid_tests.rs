/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Every rule of TOML 1.0 the subset can break, each refused as an invalid document naming the rule and the line.

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_toml::{TomlErrorEnum, TomlInvalid};

use crate::utils::refusal::refusal;

/// Each `line`, on line 2 after a line the subset reads, is refused for `reason`.
fn assert_refused_on_line_2(reason: &TomlInvalid, lines: &[&[u8]]) -> TestResult {
    for line in lines {
        let mut document = b"ok = 1\n".to_vec();
        document.extend_from_slice(line);
        document.extend_from_slice(b"\nlater = 2\n");
        assert_eq!(
            refusal(&document)?,
            TomlErrorEnum::Invalid {
                reason: reason.clone(),
                line: 2,
            },
            "{:?}",
            String::from_utf8_lossy(line)
        );
    }
    Ok(())
}

#[test]
fn test_bytes_that_are_not_utf8_are_invalid() -> TestResult {
    // 0xFF never occurs in UTF-8; 0xC3 starts a two-byte sequence the quote does not continue; 0xED 0xA0 0x80 encodes
    // the surrogate U+D800, which UTF-8 excludes.
    assert_refused_on_line_2(
        &TomlInvalid::InvalidUtf8,
        &[
            b"s = \"\xff\"",
            b"s = \"\xc3\"",
            b"# \xed\xa0\x80",
            b"\xff = 1",
        ],
    )
}

#[test]
fn test_a_control_character_other_than_tab_is_invalid() -> TestResult {
    // TOML 1.0: U+0000..U+0008, U+000A..U+001F and U+007F are refused in strings and comments; a CR only before LF.
    assert_refused_on_line_2(
        &TomlInvalid::ControlCharacter,
        &[
            b"s = \"a\x00b\"",
            b"s = \"a\x1fb\"",
            b"s = \"a\x7fb\"",
            b"# a\x01b",
            b"# a\x7f",
            b"# a\rb",
            b"s = \"a\rb\"",
            b"v = 1\r\r",
        ],
    )
}

#[test]
fn test_a_malformed_key_is_invalid() -> TestResult {
    assert_refused_on_line_2(
        &TomlInvalid::MalformedKey,
        &[
            "= 1".as_bytes(),
            "a b = 1".as_bytes(),
            "é = 1".as_bytes(),
            "a\"b\" = 1".as_bytes(),
        ],
    )
}

#[test]
fn test_a_malformed_header_is_invalid() -> TestResult {
    assert_refused_on_line_2(
        &TomlInvalid::MalformedHeader,
        &[
            "[a".as_bytes(),
            "[]".as_bytes(),
            "[ ]".as_bytes(),
            "[a b]".as_bytes(),
            "[é]".as_bytes(),
            "[ [a] ]".as_bytes(),
        ],
    )
}

#[test]
fn test_a_key_without_a_value_is_invalid() -> TestResult {
    assert_refused_on_line_2(
        &TomlInvalid::KeyWithoutValue,
        &[
            b"a",
            b"a =",
            b"a = ",
            b"a = # nothing",
            b"a # nothing",
            b"a# = 1",
        ],
    )
}

#[test]
fn test_a_value_of_no_kind_is_invalid() -> TestResult {
    assert_refused_on_line_2(
        &TomlInvalid::MalformedValue,
        &[
            b"a = yes",
            b"a = tru",
            b"a = True",
            b"a = @",
            b"a = .5",
            b"a = =1",
        ],
    )
}

#[test]
fn test_a_string_not_closed_on_its_line_is_invalid() -> TestResult {
    assert_refused_on_line_2(
        &TomlInvalid::UnterminatedString,
        &[b"a = \"abc", b"a = \"abc\\\"", b"a = \""],
    )
}

#[test]
fn test_a_malformed_integer_is_invalid() -> TestResult {
    assert_refused_on_line_2(
        &TomlInvalid::MalformedInteger,
        &[
            b"a = 1__0",
            b"a = _1",
            b"a = 1_",
            b"a = +",
            b"a = -",
            b"a = -_1",
            b"a = 12a",
            b"a = +-1",
        ],
    )
}

#[test]
fn test_an_integer_with_a_leading_zero_is_invalid() -> TestResult {
    assert_refused_on_line_2(
        &TomlInvalid::LeadingZero,
        &[b"a = 01", b"a = +01", b"a = -01", b"a = 00", b"a = 0_1"],
    )
}

#[test]
fn test_an_integer_out_of_range_is_invalid() -> TestResult {
    assert_refused_on_line_2(
        &TomlInvalid::IntegerOutOfRange,
        &[b"a = 9223372036854775808", b"a = -9223372036854775809"],
    )
}

#[test]
fn test_anything_but_a_comment_after_a_value_or_header_is_invalid() -> TestResult {
    assert_refused_on_line_2(
        &TomlInvalid::TrailingCharacters,
        &[
            b"a = 1 2",
            b"a = 1,",
            b"a = \"x\" y",
            b"a = \"\"x",
            b"a = true false",
            b"[t] b = 1",
            b"[t]]",
        ],
    )
}

#[test]
fn test_a_key_defined_twice_in_a_table_is_invalid_on_its_second_line() -> TestResult {
    assert_eq!(
        refusal(b"[storage]\nsync = true\nsync = true\n")?,
        TomlErrorEnum::Invalid {
            reason: TomlInvalid::DuplicateKey("sync".to_string()),
            line: 3,
        }
    );
    assert_eq!(
        refusal(b"a = 1\nb = 2\na = \"other\"\n")?,
        TomlErrorEnum::Invalid {
            reason: TomlInvalid::DuplicateKey("a".to_string()),
            line: 3,
        }
    );
    Ok(())
}

#[test]
fn test_a_table_named_as_a_root_key_is_invalid() -> TestResult {
    // TOML 1.0, "Table": a table and a key of one name cannot both be defined in one table.
    assert_eq!(
        refusal(b"storage = 1\n[storage]\n")?,
        TomlErrorEnum::Invalid {
            reason: TomlInvalid::DuplicateKey("storage".to_string()),
            line: 2,
        }
    );
    Ok(())
}

#[test]
fn test_a_table_with_two_headers_is_invalid_on_the_second() -> TestResult {
    assert_eq!(
        refusal(b"[a]\nx = 1\n[b]\n[ a ]\ny = 2\n")?,
        TomlErrorEnum::Invalid {
            reason: TomlInvalid::DuplicateTable("a".to_string()),
            line: 4,
        }
    );
    Ok(())
}

#[test]
fn test_the_first_refusal_in_the_document_is_reported() -> TestResult {
    assert_eq!(
        refusal(b"a = 01\nb = [1]\n")?,
        TomlErrorEnum::Invalid {
            reason: TomlInvalid::LeadingZero,
            line: 1,
        }
    );
    Ok(())
}
