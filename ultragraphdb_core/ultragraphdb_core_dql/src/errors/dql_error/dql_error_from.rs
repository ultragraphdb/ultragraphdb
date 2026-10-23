/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{LexError, LexErrorEnum};
use ultragraphdb_core_types::TypesError;

use super::{DqlError, DqlErrorEnum};

impl From<LexError> for DqlError {
    fn from(e: LexError) -> Self {
        DqlError::new(match &e.0 {
            LexErrorEnum::Lexing(m) => DqlErrorEnum::Lexing(m.clone()),
            LexErrorEnum::Located {
                line,
                column,
                message,
            } => DqlErrorEnum::Located {
                line: *line,
                column: *column,
                message: message.clone(),
            },
            _ => DqlErrorEnum::Parse(e.to_string()),
        })
    }
}

impl From<TypesError> for DqlError {
    fn from(e: TypesError) -> Self {
        DqlError::conversion(e.to_string())
    }
}
