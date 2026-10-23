/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go, dql/math.go, dql/parser_mutation.go and dql/mutation.go. Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod dql_error_display;
mod dql_error_error;
mod dql_error_from;

/// Why a DQL query, mutation or upsert block could not be parsed, or an N-Quad not converted. `Display` gives
/// Dgraph's error text, which clients match on.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DqlError(pub DqlErrorEnum);

/// The classification of a [`DqlError`]. Each variant holds the complete message Dgraph produces.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DqlErrorEnum {
    /// The lexer refused the input: `while lexing <input> at line L column C: <message>`.
    Lexing(String),
    /// The parser refused the item at a position: `line L column C: <message>`.
    Located {
        line: i32,
        column: i32,
        message: String,
    },
    /// The parser refused the input without naming a position.
    Parse(String),
    /// A uid could not be parsed or an N-Quad not converted into an edge.
    Conversion(String),
}

impl DqlError {
    pub(crate) fn new(kind: DqlErrorEnum) -> Self {
        Self(kind)
    }

    pub(crate) fn parse(msg: impl Into<String>) -> Self {
        Self::new(DqlErrorEnum::Parse(msg.into()))
    }

    pub(crate) fn conversion(msg: impl Into<String>) -> Self {
        Self::new(DqlErrorEnum::Conversion(msg.into()))
    }

    #[allow(non_snake_case)]
    pub fn Lexing(msg: String) -> Self {
        Self(DqlErrorEnum::Lexing(msg))
    }

    #[allow(non_snake_case)]
    pub fn Located(line: i32, column: i32, message: String) -> Self {
        Self(DqlErrorEnum::Located {
            line,
            column,
            message,
        })
    }

    #[allow(non_snake_case)]
    pub fn Parse(msg: String) -> Self {
        Self(DqlErrorEnum::Parse(msg))
    }

    #[allow(non_snake_case)]
    pub fn Conversion(msg: String) -> Self {
        Self(DqlErrorEnum::Conversion(msg))
    }
}
