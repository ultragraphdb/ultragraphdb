/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/strconv/number.go (NumError.Error). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt;

use super::{NumError, NumErrorEnum};

impl fmt::Display for NumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            NumErrorEnum::Syntax { func, num } => {
                write!(f, "strconv.{func}: parsing {num}: invalid syntax")
            }
            NumErrorEnum::Range { func, num } => {
                write!(f, "strconv.{func}: parsing {num}: value out of range")
            }
        }
    }
}
