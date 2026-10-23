/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/strconv/number.go (NumError). Changed for ultragraphdb: translated to Rust; the input is
 * kept as Go's strconv.Quote writes it.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod num_error_display;
mod num_error_error;

/// Go's `strconv.NumError`: a number that did not parse. `Display` gives Go's text,
/// `strconv.<func>: parsing <num>: invalid syntax` or `... value out of range`.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NumError(pub NumErrorEnum);

/// The classification of a [`NumError`]: Go's `ErrSyntax`, `ErrRange` and invalid base. `func` is the Go function that failed
/// (`ParseInt`, `ParseUint`, `ParseFloat`, `ParseBool`, `Atoi`); `num` is its input as Go's `strconv.Quote`
/// writes it, quotes and escapes included.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NumErrorEnum {
    /// The input is not a number of the syntax the function takes.
    Syntax { func: &'static str, num: String },
    /// The input is a number out of the range of the function's type.
    Range { func: &'static str, num: String },
    /// The base is neither 0 nor 2 to 36: Go's `baseError`.
    Base {
        func: &'static str,
        num: String,
        base: u32,
    },
}

impl NumError {
    #[allow(non_snake_case)]
    pub fn Syntax(func: &'static str, num: String) -> Self {
        Self(NumErrorEnum::Syntax { func, num })
    }

    #[allow(non_snake_case)]
    pub fn Range(func: &'static str, num: String) -> Self {
        Self(NumErrorEnum::Range { func, num })
    }

    #[allow(non_snake_case)]
    pub fn Base(func: &'static str, num: String, base: u32) -> Self {
        Self(NumErrorEnum::Base { func, num, base })
    }
}
