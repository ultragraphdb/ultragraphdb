/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{LexError, LexErrorEnum};

use super::{SchemaError, SchemaErrorEnum};

impl From<LexError> for SchemaError {
    fn from(e: LexError) -> Self {
        SchemaError::new(match e.kind() {
            LexErrorEnum::Lexing(m) => SchemaErrorEnum::Lexing(m.clone()),
            LexErrorEnum::Located {
                line,
                column,
                message,
            } => SchemaErrorEnum::Located {
                line: *line,
                column: *column,
                message: message.clone(),
            },
            _ => SchemaErrorEnum::Parse(e.to_string()),
        })
    }
}
