/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Each feature of TOML 1.0 outside the subset, on a line of its own, refused by name with its line.

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_toml::{TomlErrorEnum, TomlFeature};

use crate::utils::refusal::refusal;

/// The features of keys and strings, each with lines using it, from the TOML 1.0 specification's own examples
/// where it has them, as the other cases are.
fn key_cases() -> Vec<(TomlFeature, Vec<&'static str>)> {
    vec![
        (
            TomlFeature::DottedKeys,
            vec![
                "physical.color = \"orange\"",
                "a . b = 1",
                "1.2 = 3",
                "[dog.tater]",
                "[ a . b ]",
            ],
        ),
        (
            TomlFeature::QuotedKeys,
            vec![
                "\"127.0.0.1\" = \"value\"",
                "'key2' = \"value\"",
                "\"\" = 1",
                "[\"key#group\"]",
                "[ 'a' ]",
            ],
        ),
        (
            TomlFeature::LiteralStrings,
            vec!["winpath = 'C:\\Users\\nodejs\\templates'", "s = ''"],
        ),
        (
            TomlFeature::MultiLineStrings,
            vec![
                "str1 = \"\"\"",
                "s = \"\"\"Roses are red\"\"\"",
                "regex2 = '''I [dw]on't need \\d{2} apples'''",
                "s = '''",
            ],
        ),
    ]
}

/// The features of numbers, dates and structures, each with lines using it.
fn value_cases() -> Vec<(TomlFeature, Vec<&'static str>)> {
    vec![
        (
            TomlFeature::Floats,
            vec![
                "ratio = 0.5",
                "flt1 = +1.0",
                "flt4 = 5e+22",
                "flt5 = 1e06",
                "flt6 = -2E-2",
                "flt8 = 224_617.445_991_228",
            ],
        ),
        (
            TomlFeature::InfinitiesAndNan,
            vec![
                "sf1 = inf",
                "sf2 = +inf",
                "sf3 = -inf",
                "sf4 = nan",
                "sf5 = +nan",
                "sf6 = -nan",
            ],
        ),
        (
            TomlFeature::DatesAndTimes,
            vec![
                "odt1 = 1979-05-27T07:32:00Z",
                "odt2 = 1979-05-27T00:32:00-07:00",
                "odt4 = 1979-05-27 07:32:00Z",
                "ldt1 = 1979-05-27T07:32:00",
                "ld1 = 1979-05-27",
                "lt1 = 07:32:00",
                "lt2 = 00:32:00.999999",
            ],
        ),
        (
            TomlFeature::Arrays,
            vec![
                "peers = [\"a\", \"b\"]",
                "integers = [ 1, 2, 3 ]",
                "empty = []",
                "nested = [",
            ],
        ),
        (
            TomlFeature::InlineTables,
            vec![
                "name = { first = \"Tom\", last = \"Preston-Werner\" }",
                "empty = {}",
            ],
        ),
        (TomlFeature::ArraysOfTables, vec!["[[products]]", "[[ a ]]"]),
        (
            TomlFeature::HexadecimalIntegers,
            vec!["hex1 = 0xDEADBEEF", "hex3 = 0xdead_beef", "h = 0x"],
        ),
        (
            TomlFeature::OctalIntegers,
            vec!["oct1 = 0o01234567", "oct2 = 0o755"],
        ),
        (
            TomlFeature::BinaryIntegers,
            vec!["bin1 = 0b11010110", "b = 0b1_0"],
        ),
    ]
}

#[test]
fn test_each_unsupported_feature_is_refused_by_name_and_line() -> TestResult {
    for (feature, lines) in key_cases().into_iter().chain(value_cases()) {
        for line in lines {
            // Line 2, after a line the subset reads, so the error's line is not the first by accident.
            let document = format!("ok = 1\n{line}\nlater = 2\n");
            assert_eq!(
                refusal(document.as_bytes())?,
                TomlErrorEnum::Unsupported { feature, line: 2 },
                "{line}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_an_unsupported_feature_in_a_table_is_refused_on_its_line() -> TestResult {
    assert_eq!(
        refusal(b"name = \"dev\"\n\n[transport]\n# peers\npeers = [\"a\"]\n")?,
        TomlErrorEnum::Unsupported {
            feature: TomlFeature::Arrays,
            line: 5,
        }
    );
    Ok(())
}
