/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/parse.go and schema/schema.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod schema_error_display;
mod schema_error_error;
mod schema_error_from;

/// Why schema text could not be parsed, or the schema state could not answer. `Display` gives Dgraph's error text,
/// which clients match on.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchemaError(pub SchemaErrorEnum);

/// The classification of a [`SchemaError`]. Each variant holds the complete message Dgraph produces.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SchemaErrorEnum {
    /// The lexer refused the text: `while lexing <input> at line L column C: <message>`.
    Lexing(String),
    /// The parser refused the item at a position: `line L column C: <message>`.
    Located {
        line: i32,
        column: i32,
        message: String,
    },
    /// The parser refused the text without naming a position, e.g. a namespace or the tokenizer check.
    Parse(String),
    /// The schema state holds no definition for the predicate, or one it cannot read.
    State(String),
}

impl SchemaError {
    pub(crate) fn new(kind: SchemaErrorEnum) -> Self {
        Self(kind)
    }

    pub(crate) fn parse(msg: impl Into<String>) -> Self {
        Self::new(SchemaErrorEnum::Parse(msg.into()))
    }

    pub(crate) fn state(msg: impl Into<String>) -> Self {
        Self::new(SchemaErrorEnum::State(msg.into()))
    }

    /// Go's `errors.Wrapf(err, prefix)`: `<prefix>: <err>`, a positionless error.
    pub(crate) fn wrap(self, prefix: &str) -> Self {
        Self::parse(format!("{prefix}: {self}"))
    }

    #[allow(non_snake_case)]
    pub fn Lexing(msg: String) -> Self {
        Self(SchemaErrorEnum::Lexing(msg))
    }

    #[allow(non_snake_case)]
    pub fn Located(line: i32, column: i32, message: String) -> Self {
        Self(SchemaErrorEnum::Located {
            line,
            column,
            message,
        })
    }

    #[allow(non_snake_case)]
    pub fn Parse(msg: String) -> Self {
        Self(SchemaErrorEnum::Parse(msg))
    }

    #[allow(non_snake_case)]
    pub fn State(msg: String) -> Self {
        Self(SchemaErrorEnum::State(msg))
    }
}
