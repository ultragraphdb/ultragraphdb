/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/conversion.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod types_error_display;
mod types_error_error;
mod types_error_from;

/// Why a scalar value could not be converted, parsed, compared, sorted or indexed. `Display` gives Dgraph's error
/// text, which clients match on.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypesError(pub TypesErrorEnum);

/// The classification of a [`TypesError`]. Each variant holds the complete message Dgraph produces.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypesErrorEnum {
    /// `Convert`, `Marshal` or `ObjectValue` refused the value or the pair of types.
    Conversion(String),
    /// Go's `strconv.ParseInt`, `ParseFloat` or `ParseBool` refused the text.
    Number(String),
    /// Go's `time` package refused a text or binary datetime.
    Time(String),
    /// A text is no `xsd:decimal`, or a value lies outside a decimal's range.
    Decimal(String),
    /// A geometry could not be decoded, encoded, indexed or queried.
    Geo(String),
    /// A password was too short or did not match its hash.
    Password(String),
    /// Values could not be compared or sorted.
    Compare(String),
}

impl TypesError {
    /// The message, as `Display` writes it.
    pub fn message(&self) -> &str {
        match &self.0 {
            TypesErrorEnum::Conversion(m)
            | TypesErrorEnum::Number(m)
            | TypesErrorEnum::Time(m)
            | TypesErrorEnum::Decimal(m)
            | TypesErrorEnum::Geo(m)
            | TypesErrorEnum::Password(m)
            | TypesErrorEnum::Compare(m) => m,
        }
    }

    /// A [`TypesErrorEnum::Conversion`] error with its message.
    pub fn conversion(msg: impl Into<String>) -> Self {
        Self(TypesErrorEnum::Conversion(msg.into()))
    }

    /// A [`TypesErrorEnum::Number`] error with its message.
    pub fn number(msg: impl Into<String>) -> Self {
        Self(TypesErrorEnum::Number(msg.into()))
    }

    /// A [`TypesErrorEnum::Time`] error with its message.
    pub fn time(msg: impl Into<String>) -> Self {
        Self(TypesErrorEnum::Time(msg.into()))
    }

    /// A [`TypesErrorEnum::Decimal`] error with its message.
    pub fn decimal(msg: impl Into<String>) -> Self {
        Self(TypesErrorEnum::Decimal(msg.into()))
    }

    /// A [`TypesErrorEnum::Geo`] error with its message.
    pub fn geo(msg: impl Into<String>) -> Self {
        Self(TypesErrorEnum::Geo(msg.into()))
    }

    /// A [`TypesErrorEnum::Password`] error with its message.
    pub fn password(msg: impl Into<String>) -> Self {
        Self(TypesErrorEnum::Password(msg.into()))
    }

    /// A [`TypesErrorEnum::Compare`] error with its message.
    pub fn compare(msg: impl Into<String>) -> Self {
        Self(TypesErrorEnum::Compare(msg.into()))
    }
}
