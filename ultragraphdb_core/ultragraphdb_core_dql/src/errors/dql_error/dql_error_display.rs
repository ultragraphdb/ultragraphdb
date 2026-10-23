/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::{DqlError, DqlErrorEnum};

impl fmt::Display for DqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            DqlErrorEnum::Lexing(m) | DqlErrorEnum::Parse(m) | DqlErrorEnum::Conversion(m) => {
                f.write_str(m)
            }
            DqlErrorEnum::Located {
                line,
                column,
                message,
            } => {
                write!(f, "line {line} column {column}: {message}")
            }
        }
    }
}
