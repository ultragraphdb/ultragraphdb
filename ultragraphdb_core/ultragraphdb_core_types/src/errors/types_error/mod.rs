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
    /// Go's `math/big` refused a big-float text.
    BigFloat(String),
    /// A geometry could not be decoded, encoded, indexed or queried.
    Geo(String),
    /// A password was too short or did not match its hash.
    Password(String),
    /// Values could not be compared or sorted.
    Compare(String),
}

impl TypesError {
    pub(crate) fn new(kind: TypesErrorEnum) -> Self {
        Self(kind)
    }

    pub(crate) fn conversion(msg: impl Into<String>) -> Self {
        Self::new(TypesErrorEnum::Conversion(msg.into()))
    }

    pub(crate) fn number(msg: impl Into<String>) -> Self {
        Self::new(TypesErrorEnum::Number(msg.into()))
    }

    pub(crate) fn time(msg: impl Into<String>) -> Self {
        Self::new(TypesErrorEnum::Time(msg.into()))
    }

    pub(crate) fn big_float(msg: impl Into<String>) -> Self {
        Self::new(TypesErrorEnum::BigFloat(msg.into()))
    }

    pub(crate) fn geo(msg: impl Into<String>) -> Self {
        Self::new(TypesErrorEnum::Geo(msg.into()))
    }

    pub(crate) fn password(msg: impl Into<String>) -> Self {
        Self::new(TypesErrorEnum::Password(msg.into()))
    }

    pub(crate) fn compare(msg: impl Into<String>) -> Self {
        Self::new(TypesErrorEnum::Compare(msg.into()))
    }

    /// The message, as `Display` writes it.
    pub fn message(&self) -> &str {
        match &self.0 {
            TypesErrorEnum::Conversion(m)
            | TypesErrorEnum::Number(m)
            | TypesErrorEnum::Time(m)
            | TypesErrorEnum::BigFloat(m)
            | TypesErrorEnum::Geo(m)
            | TypesErrorEnum::Password(m)
            | TypesErrorEnum::Compare(m) => m,
        }
    }

    /// The same classification with `prefix: ` before the message, as Go's `errors.Wrapf` writes it.
    pub(crate) fn wrap(self, prefix: &str) -> Self {
        let msg = format!("{prefix}: {}", self.message());
        Self::new(match self.0 {
            TypesErrorEnum::Conversion(_) => TypesErrorEnum::Conversion(msg),
            TypesErrorEnum::Number(_) => TypesErrorEnum::Number(msg),
            TypesErrorEnum::Time(_) => TypesErrorEnum::Time(msg),
            TypesErrorEnum::BigFloat(_) => TypesErrorEnum::BigFloat(msg),
            TypesErrorEnum::Geo(_) => TypesErrorEnum::Geo(msg),
            TypesErrorEnum::Password(_) => TypesErrorEnum::Password(msg),
            TypesErrorEnum::Compare(_) => TypesErrorEnum::Compare(msg),
        })
    }

    #[allow(non_snake_case)]
    pub fn Conversion(msg: String) -> Self {
        Self(TypesErrorEnum::Conversion(msg))
    }

    #[allow(non_snake_case)]
    pub fn Number(msg: String) -> Self {
        Self(TypesErrorEnum::Number(msg))
    }

    #[allow(non_snake_case)]
    pub fn Time(msg: String) -> Self {
        Self(TypesErrorEnum::Time(msg))
    }

    #[allow(non_snake_case)]
    pub fn BigFloat(msg: String) -> Self {
        Self(TypesErrorEnum::BigFloat(msg))
    }

    #[allow(non_snake_case)]
    pub fn Geo(msg: String) -> Self {
        Self(TypesErrorEnum::Geo(msg))
    }

    #[allow(non_snake_case)]
    pub fn Password(msg: String) -> Self {
        Self(TypesErrorEnum::Password(msg))
    }

    #[allow(non_snake_case)]
    pub fn Compare(msg: String) -> Self {
        Self(TypesErrorEnum::Compare(msg))
    }
}
