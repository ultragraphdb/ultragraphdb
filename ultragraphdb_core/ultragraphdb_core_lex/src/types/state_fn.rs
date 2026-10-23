/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::lexer::Lexer;

/// A state of the lexer: a function that lexes from the current position and returns the next state, or `None`
/// to stop. Go's `StateFn`; a newtype because a function pointer type cannot name itself.
#[derive(Debug, Clone, Copy)]
pub struct StateFn(fn(&mut Lexer<'_>) -> Option<StateFn>);

impl StateFn {
    pub const fn new(f: fn(&mut Lexer<'_>) -> Option<StateFn>) -> Self {
        Self(f)
    }

    pub(crate) fn call(self, l: &mut Lexer<'_>) -> Option<StateFn> {
        (self.0)(l)
    }
}
