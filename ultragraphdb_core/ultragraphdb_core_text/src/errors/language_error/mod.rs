/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/parse.go, internal/language/language.go and
 * internal/language/match.go (the error values). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod language_error_display;
mod language_error_error;

/// Why a language tag could not be parsed or changed, with `golang.org/x/text/language`'s error texts.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LanguageError(pub LanguageErrorEnum);

/// Detailed classification of language tag errors.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LanguageErrorEnum {
    /// The tag is not well-formed BCP 47 (Go's `ErrSyntax`).
    Syntax,
    /// A `-u` extension has the same key twice with different values (Go's `ErrDuplicateKey`).
    DuplicateKey,
    /// A subtag is well-formed but unknown (Go's `ValueError`); holds its first eight bytes.
    Value(Vec<u8>),
    /// A key was set on a private-use tag.
    PrivateUse,
    /// A key or type of the wrong length was set.
    InvalidArguments,
    /// No likely subtags are known for the tag.
    MissingLikelyTagsData,
}

impl LanguageError {
    pub(crate) fn new(variant: LanguageErrorEnum) -> Self {
        Self(variant)
    }

    /// Go's `NewValueError`: the subtag's first eight bytes, up to a zero byte.
    pub(crate) fn unknown_subtag(tag: &[u8]) -> Self {
        let tag = &tag[..tag.len().min(8)];
        let end = tag.iter().position(|&c| c == 0).unwrap_or(tag.len());
        Self::new(LanguageErrorEnum::Value(tag[..end].to_vec()))
    }

    pub(crate) fn is_syntax(&self) -> bool {
        self.0 == LanguageErrorEnum::Syntax
    }

    /// A [`LanguageErrorEnum::Syntax`] error: the tag is not well-formed BCP 47.
    #[must_use]
    pub fn syntax() -> Self {
        Self(LanguageErrorEnum::Syntax)
    }

    /// A [`LanguageErrorEnum::DuplicateKey`] error: a `-u` extension has the same key twice with different values.
    #[must_use]
    pub fn duplicate_key() -> Self {
        Self(LanguageErrorEnum::DuplicateKey)
    }

    /// A [`LanguageErrorEnum::Value`] error: a well-formed but unknown subtag, `value`.
    #[must_use]
    pub fn value(value: Vec<u8>) -> Self {
        Self(LanguageErrorEnum::Value(value))
    }

    /// A [`LanguageErrorEnum::PrivateUse`] error: a key was set on a private-use tag.
    #[must_use]
    pub fn private_use() -> Self {
        Self(LanguageErrorEnum::PrivateUse)
    }

    /// A [`LanguageErrorEnum::InvalidArguments`] error: a key or type of the wrong length was set.
    #[must_use]
    pub fn invalid_arguments() -> Self {
        Self(LanguageErrorEnum::InvalidArguments)
    }

    /// A [`LanguageErrorEnum::MissingLikelyTagsData`] error: no likely subtags are known for the tag.
    #[must_use]
    pub fn missing_likely_tags_data() -> Self {
        Self(LanguageErrorEnum::MissingLikelyTagsData)
    }
}
