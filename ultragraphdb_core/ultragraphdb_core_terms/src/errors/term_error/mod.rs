/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod term_error_display;
mod term_error_error;

/// Why a term could not be encoded or an encoding could not be read. The classification is the public field `.0`;
/// branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TermError(pub TermErrorEnum);

/// The classification of a [`TermError`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TermErrorEnum {
    /// The bytes end before the encoding their tag announces does.
    Truncated { bytes: Vec<u8> },
    /// The first byte is no tag an encoding begins with.
    UnknownTag { tag: u8 },
    /// A known-datatype code no datatype has.
    UnknownDatatype { code: u8 },
    /// An inline lexical form or language tag that is not UTF-8, or longer than an inline one may be.
    Malformed {
        bytes: Vec<u8>,
        reason: &'static str,
    },
    /// A vector literal not in its canonical form, empty, longer than 65,535 elements, or with an element that is not
    /// finite.
    InvalidVector {
        lexical: String,
        reason: &'static str,
    },
    /// An encoding of a UID where a literal was asked for.
    NotALiteral { uid: u64 },
}

impl TermError {
    #[allow(non_snake_case)]
    pub fn Truncated(bytes: Vec<u8>) -> Self {
        Self(TermErrorEnum::Truncated { bytes })
    }

    #[allow(non_snake_case)]
    pub fn UnknownTag(tag: u8) -> Self {
        Self(TermErrorEnum::UnknownTag { tag })
    }

    #[allow(non_snake_case)]
    pub fn UnknownDatatype(code: u8) -> Self {
        Self(TermErrorEnum::UnknownDatatype { code })
    }

    #[allow(non_snake_case)]
    pub fn Malformed(bytes: Vec<u8>, reason: &'static str) -> Self {
        Self(TermErrorEnum::Malformed { bytes, reason })
    }

    #[allow(non_snake_case)]
    pub fn InvalidVector(lexical: String, reason: &'static str) -> Self {
        Self(TermErrorEnum::InvalidVector { lexical, reason })
    }

    #[allow(non_snake_case)]
    pub fn NotALiteral(uid: u64) -> Self {
        Self(TermErrorEnum::NotALiteral { uid })
    }
}
