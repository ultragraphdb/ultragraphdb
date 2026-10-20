/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/tok.go and tok/tokens.go (their errors). Changed for ultragraphdb: translated to
 * Rust; the errors Go returns as `errors.Errorf`, and the type assertions Go panics on, as one type.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod tok_error_display;
mod tok_error_error;

use ultragraphdb_core_types::TypesError;

use crate::errors::options_error::OptionsError;

/// Why a tokenizer, a tokenizer lookup or a vector index specification failed. `Display` gives Dgraph's error
/// text, which clients match on.
///
/// The wrapped classification is private; branch on [`TokError::kind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokError(TokErrorEnum);

/// The classification of a [`TokError`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokErrorEnum {
    /// A tokenizer name or identifier is not registered: `Invalid tokenizer <name>`,
    /// `No tokenizer was found with id <id>`.
    UnknownTokenizer(String),
    /// A tokenizer was given a value it does not take. Go returns an error for the string tokenizers that check
    /// (`Exact indices only supported for string types`, ...) and panics on the type assertion of the others; this
    /// port returns the panic's text as an error.
    WrongValue(String),
    /// The geo index refused the geometry; the message is `ultragraphdb_core_types`'.
    Geo(TypesError),
    /// A token function got the wrong number of arguments: `Function requires 1 arguments, but got <n>`.
    Arguments(String),
    /// A vector index specification names no registered factory.
    UnknownFactory(String),
    /// A vector index option was refused.
    Options(OptionsError),
}

impl TokError {
    pub(crate) fn new(kind: TokErrorEnum) -> Self {
        Self(kind)
    }

    /// Classification, for callers that need to branch on the failure.
    pub fn kind(&self) -> &TokErrorEnum {
        &self.0
    }

    pub(crate) fn unknown_tokenizer(msg: impl Into<String>) -> Self {
        Self::new(TokErrorEnum::UnknownTokenizer(msg.into()))
    }

    pub(crate) fn wrong_value(msg: impl Into<String>) -> Self {
        Self::new(TokErrorEnum::WrongValue(msg.into()))
    }

    pub(crate) fn arguments(msg: impl Into<String>) -> Self {
        Self::new(TokErrorEnum::Arguments(msg.into()))
    }

    pub(crate) fn unknown_factory(msg: impl Into<String>) -> Self {
        Self::new(TokErrorEnum::UnknownFactory(msg.into()))
    }
}

impl From<TypesError> for TokError {
    fn from(e: TypesError) -> Self {
        Self::new(TokErrorEnum::Geo(e))
    }
}

impl From<OptionsError> for TokError {
    fn from(e: OptionsError) -> Self {
        Self::new(TokErrorEnum::Options(e))
    }
}
