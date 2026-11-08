/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

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
    Truncated {
        /// The bytes read.
        bytes: Vec<u8>,
    },
    /// The first byte is no tag an encoding begins with.
    UnknownTag {
        /// The first byte.
        tag: u8,
    },
    /// A known-datatype code no datatype has.
    UnknownDatatype {
        /// The code read.
        code: u8,
    },
    /// An inline lexical form or language tag that is not UTF-8, or longer than an inline one may be.
    Malformed {
        /// The bytes of the form or tag.
        bytes: Vec<u8>,
        /// What is wrong with them.
        reason: &'static str,
    },
    /// A vector literal not in its canonical form, empty, longer than 65,535 elements, or with an element that is not
    /// finite.
    InvalidVector {
        /// The vector's lexical form.
        lexical: String,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// An encoding of an IRI's or a blank node's system ID where a literal was asked for.
    NotALiteral {
        /// The ID encoded.
        id: SystemId,
    },
}

impl TermError {
    /// A [`TermErrorEnum::Truncated`] for `bytes`.
    #[must_use]
    pub fn truncated(bytes: Vec<u8>) -> Self {
        Self(TermErrorEnum::Truncated { bytes })
    }

    /// A [`TermErrorEnum::UnknownTag`] for `tag`.
    #[must_use]
    pub fn unknown_tag(tag: u8) -> Self {
        Self(TermErrorEnum::UnknownTag { tag })
    }

    /// A [`TermErrorEnum::UnknownDatatype`] for `code`.
    #[must_use]
    pub fn unknown_datatype(code: u8) -> Self {
        Self(TermErrorEnum::UnknownDatatype { code })
    }

    /// A [`TermErrorEnum::Malformed`] for `bytes`, saying why.
    #[must_use]
    pub fn malformed(bytes: Vec<u8>, reason: &'static str) -> Self {
        Self(TermErrorEnum::Malformed { bytes, reason })
    }

    /// A [`TermErrorEnum::InvalidVector`] for `lexical`, saying why.
    #[must_use]
    pub fn invalid_vector(lexical: String, reason: &'static str) -> Self {
        Self(TermErrorEnum::InvalidVector { lexical, reason })
    }

    /// A [`TermErrorEnum::NotALiteral`] for `id`.
    #[must_use]
    pub fn not_a_literal(id: SystemId) -> Self {
        Self(TermErrorEnum::NotALiteral { id })
    }
}
