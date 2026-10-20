/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/options/allowed_options.go, option_parser.go, options.go and
 * tok/hnsw/persistent_factory.go (their errors). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod options_error_display;
mod options_error_error;

/// Why an index option was refused. `Display` gives Dgraph's error text, which the schema parser wraps into its
/// own message.
///
/// The wrapped classification is private; branch on [`OptionsError::kind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsError(OptionsErrorEnum);

/// The classification of an [`OptionsError`]. Each variant holds the complete message Dgraph produces.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptionsErrorEnum {
    /// The option is not among the allowed options: `option <name> is not allowed!`.
    NotAllowed(String),
    /// The option's text does not parse as its type, with Go's `strconv` message.
    Parse(String),
    /// The custom parser refused the value, e.g. `Can't create a vector index for <metric>`.
    Invalid(String),
    /// A stored value is not of the type asked for: `unable to assign a value of type <T> to type <U>`.
    Type(String),
}

impl OptionsError {
    pub(crate) fn new(kind: OptionsErrorEnum) -> Self {
        Self(kind)
    }

    /// A refusal by a custom option parser, with its message.
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::new(OptionsErrorEnum::Invalid(msg.into()))
    }

    /// Classification, for callers that need to branch on the failure.
    pub fn kind(&self) -> &OptionsErrorEnum {
        &self.0
    }

    /// The message, as `Display` writes it.
    pub fn message(&self) -> &str {
        match &self.0 {
            OptionsErrorEnum::NotAllowed(m)
            | OptionsErrorEnum::Parse(m)
            | OptionsErrorEnum::Invalid(m)
            | OptionsErrorEnum::Type(m) => m,
        }
    }
}
