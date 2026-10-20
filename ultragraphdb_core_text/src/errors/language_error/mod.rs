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
/// The wrapped classification is private; branch on [`LanguageError::kind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageError(LanguageErrorEnum);

/// Detailed classification of language tag errors.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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

    pub(crate) fn syntax() -> Self {
        Self::new(LanguageErrorEnum::Syntax)
    }

    /// Go's `NewValueError`: the subtag's first eight bytes, up to a zero byte.
    pub(crate) fn value(tag: &[u8]) -> Self {
        let tag = &tag[..tag.len().min(8)];
        let end = tag.iter().position(|&c| c == 0).unwrap_or(tag.len());
        Self::new(LanguageErrorEnum::Value(tag[..end].to_vec()))
    }

    pub(crate) fn is_syntax(&self) -> bool {
        self.0 == LanguageErrorEnum::Syntax
    }

    /// Classification, for callers that need to branch on the failure.
    pub fn kind(&self) -> &LanguageErrorEnum {
        &self.0
    }
}
