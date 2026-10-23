/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::Lexer;
use crate::errors::lex_error::{LexError, LexErrorEnum};
use crate::types::item_iterator::ItemIterator;
use crate::types::item_type::ITEM_ERROR;
use crate::types::state_fn::StateFn;

impl<'a> Lexer<'a> {
    /// Runs the state functions from `f` until one returns `None`.
    pub fn run(&mut self, f: StateFn) -> &mut Self {
        let mut state = Some(f);
        while let Some(s) = state {
            state = s.call(self);
        }
        self
    }

    /// Whether the input lexed without errors.
    ///
    /// # Errors
    ///
    /// [`LexErrorEnum::Lexing`] with the text of the first error item.
    pub fn validate_result(&self) -> Result<(), LexError> {
        match self.items.iter().find(|item| item.typ() == ITEM_ERROR) {
            Some(item) => Err(LexError::new(LexErrorEnum::Lexing(item.val().to_string()))),
            None => Ok(()),
        }
    }

    /// An iterator over the emitted items, before the first.
    pub fn new_iterator(&self) -> ItemIterator<'_, 'a> {
        ItemIterator::new(&self.items)
    }
}
