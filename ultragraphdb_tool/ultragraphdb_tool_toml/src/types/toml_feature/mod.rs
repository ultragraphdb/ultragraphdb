/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod toml_feature_display;

/// A feature of TOML 1.0 outside the subset the settings use, which the parser refuses by name rather than reading
/// in part or as something else. Its text is the feature's name in the plural, as an error names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TomlFeature {
    /// A key of several parts joined by `.`, as `a.b = 1` or `[a.b]`.
    DottedKeys,
    /// A key in quotes, as `"a" = 1`, `'a' = 1` or `["a"]`.
    QuotedKeys,
    /// A string in single quotes, as `'a'`.
    LiteralStrings,
    /// A string in three quotes of either kind, as `"""a"""` or `'''a'''`.
    MultiLineStrings,
    /// A number with a fraction or an exponent, as `0.5` or `1e3`.
    Floats,
    /// The floats `inf` and `nan`, with or without a sign.
    InfinitiesAndNan,
    /// An offset or local date-time, a local date or a local time, as `1979-05-27` or `07:32:00`.
    DatesAndTimes,
    /// A value in brackets, as `["a", "b"]`.
    Arrays,
    /// A table in braces, as `{ a = 1 }`.
    InlineTables,
    /// A header in double brackets, as `[[a]]`.
    ArraysOfTables,
    /// An integer prefixed `0x`.
    HexadecimalIntegers,
    /// An integer prefixed `0o`.
    OctalIntegers,
    /// An integer prefixed `0b`.
    BinaryIntegers,
}
