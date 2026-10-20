/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/internal/strconv/atoi.go (NumError). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt;

use crate::utils::go_quote::quote;

/// Go's `strconv.NumError`: the function, its input and whether the number was out of range rather than
/// malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NumError {
    pub func: &'static str,
    pub num: String,
    pub range: bool,
}

impl fmt::Display for NumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let why = if self.range {
            "value out of range"
        } else {
            "invalid syntax"
        };
        write!(
            f,
            "strconv.{}: parsing {}: {why}",
            self.func,
            quote(&self.num)
        )
    }
}
