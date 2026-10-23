/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, schema/parse.go and schema/schema.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt;

use super::{SchemaError, SchemaErrorEnum};

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            SchemaErrorEnum::Lexing(m)
            | SchemaErrorEnum::Parse(m)
            | SchemaErrorEnum::State(m)
            | SchemaErrorEnum::Key(m) => f.write_str(m),
            SchemaErrorEnum::Located {
                line,
                column,
                message,
            } => write!(f, "line {line} column {column}: {message}"),
        }
    }
}
