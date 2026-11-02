/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_toml::TomlFeature;

#[test]
fn test_each_feature_is_named_in_the_plural() {
    let names = [
        (TomlFeature::DottedKeys, "dotted keys"),
        (TomlFeature::QuotedKeys, "quoted keys"),
        (TomlFeature::LiteralStrings, "literal strings"),
        (TomlFeature::MultiLineStrings, "multi-line strings"),
        (TomlFeature::Floats, "floats"),
        (TomlFeature::InfinitiesAndNan, "infinities and NaN"),
        (
            TomlFeature::DatesAndTimes,
            "offset and local dates and times",
        ),
        (TomlFeature::Arrays, "arrays"),
        (TomlFeature::InlineTables, "inline tables"),
        (TomlFeature::ArraysOfTables, "arrays of tables"),
        (TomlFeature::HexadecimalIntegers, "hexadecimal integers"),
        (TomlFeature::OctalIntegers, "octal integers"),
        (TomlFeature::BinaryIntegers, "binary integers"),
    ];
    for (feature, name) in names {
        assert_eq!(feature.to_string(), name);
    }
}
