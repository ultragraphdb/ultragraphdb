/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::KeyError;
use ultragraphdb_core_lex::{LexError, LexErrorEnum};
use ultragraphdb_error_store::StoreError;

use super::{SchemaError, SchemaErrorEnum};

impl From<LexError> for SchemaError {
    fn from(e: LexError) -> Self {
        SchemaError::new(match &e.0 {
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

impl From<KeyError> for SchemaError {
    fn from(e: KeyError) -> Self {
        SchemaError::new(SchemaErrorEnum::Key(e.to_string()))
    }
}

impl From<StoreError> for SchemaError {
    fn from(e: StoreError) -> Self {
        SchemaError::new(SchemaErrorEnum::Store(e))
    }
}
