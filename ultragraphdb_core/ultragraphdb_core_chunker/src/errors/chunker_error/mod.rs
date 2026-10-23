/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/rdf_parser.go and chunker/json_parser.go. Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod chunker_error_display;
mod chunker_error_error;

use ultragraphdb_core_lex::LexError;
use ultragraphdb_core_types::TypesError;

/// Why an RDF line or a JSON mutation could not be parsed into N-Quads. `Display` gives Dgraph's error text, which
/// clients match on.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChunkerError(pub ChunkerErrorEnum);

/// The classification of a [`ChunkerError`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChunkerErrorEnum {
    /// Dgraph's `ErrEmpty`, a harmless error: the line is empty or a comment, `RDF: harmless error, e.g. comment
    /// line`.
    Empty,
    /// The RDF line did not lex; the lexer's text, `while lexing <line> at line L column C: <message>`.
    Lex(LexError),
    /// The RDF line lexed but is not an N-Quad Dgraph takes, with Dgraph's text, e.g. `invalid end of input. Input:
    /// [<line>]` or `could not parse facet: <why>`.
    Rdf(String),
    /// A typed literal's value does not convert to its type, with the conversion's text.
    Types(TypesError),
    /// The input is not JSON, or its root is neither an object nor an array; serde_json's text where Go's
    /// encoding/json has its own.
    Json(String),
    /// A JSON object to delete has no uid: `UID must be present and non-zero while deleting edges`.
    EmptyUid,
    /// The JSON is not a mutation Dgraph takes, with Dgraph's text, e.g. `only array of map allowed at root` or a
    /// uid that does not parse.
    JsonMutation(String),
}

impl ChunkerError {
    pub(crate) fn new(kind: ChunkerErrorEnum) -> Self {
        Self(kind)
    }

    pub(crate) fn rdf(msg: impl Into<String>) -> Self {
        Self::new(ChunkerErrorEnum::Rdf(msg.into()))
    }

    pub(crate) fn json_mutation(msg: impl Into<String>) -> Self {
        Self::new(ChunkerErrorEnum::JsonMutation(msg.into()))
    }

    #[allow(non_snake_case)]
    pub fn Empty() -> Self {
        Self(ChunkerErrorEnum::Empty)
    }

    #[allow(non_snake_case)]
    pub fn Lex(value: LexError) -> Self {
        Self(ChunkerErrorEnum::Lex(value))
    }

    #[allow(non_snake_case)]
    pub fn Rdf(msg: String) -> Self {
        Self(ChunkerErrorEnum::Rdf(msg))
    }

    #[allow(non_snake_case)]
    pub fn Types(value: TypesError) -> Self {
        Self(ChunkerErrorEnum::Types(value))
    }

    #[allow(non_snake_case)]
    pub fn Json(msg: String) -> Self {
        Self(ChunkerErrorEnum::Json(msg))
    }

    #[allow(non_snake_case)]
    pub fn EmptyUid() -> Self {
        Self(ChunkerErrorEnum::EmptyUid)
    }

    #[allow(non_snake_case)]
    pub fn JsonMutation(msg: String) -> Self {
        Self(ChunkerErrorEnum::JsonMutation(msg))
    }
}

impl From<LexError> for ChunkerError {
    fn from(e: LexError) -> Self {
        Self::new(ChunkerErrorEnum::Lex(e))
    }
}

impl From<TypesError> for ChunkerError {
    fn from(e: TypesError) -> Self {
        Self::new(ChunkerErrorEnum::Types(e))
    }
}

impl From<serde_json::Error> for ChunkerError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(ChunkerErrorEnum::Json(e.to_string()))
    }
}
