/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::tok_error::{TokError, TokErrorEnum};

impl fmt::Display for TokError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TokErrorEnum::UnknownTokenizer(m)
            | TokErrorEnum::WrongValue(m)
            | TokErrorEnum::Arguments(m)
            | TokErrorEnum::UnknownFactory(m) => f.write_str(m),
            TokErrorEnum::Geo(e) => e.fmt(f),
            TokErrorEnum::Options(e) => e.fmt(f),
        }
    }
}
