/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::toml_feature::TomlFeature;

impl fmt::Display for TomlFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::DottedKeys => "dotted keys",
            Self::QuotedKeys => "quoted keys",
            Self::LiteralStrings => "literal strings",
            Self::MultiLineStrings => "multi-line strings",
            Self::Floats => "floats",
            Self::InfinitiesAndNan => "infinities and NaN",
            Self::DatesAndTimes => "offset and local dates and times",
            Self::Arrays => "arrays",
            Self::InlineTables => "inline tables",
            Self::ArraysOfTables => "arrays of tables",
            Self::HexadecimalIntegers => "hexadecimal integers",
            Self::OctalIntegers => "octal integers",
            Self::BinaryIntegers => "binary integers",
        };
        f.write_str(name)
    }
}
