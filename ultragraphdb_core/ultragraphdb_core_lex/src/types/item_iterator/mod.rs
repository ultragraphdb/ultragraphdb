/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt::Display;

use crate::errors::lex_error::{LexError, LexErrorEnum};
use crate::types::item::Item;

static OUT_OF_RANGE: Item<'static> = Item::out_of_range();

/// A cursor over the items a lexer emitted. It starts before the first item; [`ItemIterator::next`] moves onto
/// the next one. Items borrow the lexer, not the iterator, so they stay usable while the cursor moves.
#[derive(Debug, Clone)]
pub struct ItemIterator<'l, 'a> {
    items: &'l [Item<'a>],
    idx: isize,
}

impl<'l, 'a> ItemIterator<'l, 'a> {
    pub(crate) fn new(items: &'l [Item<'a>]) -> Self {
        Self { items, idx: -1 }
    }

    fn len(&self) -> isize {
        self.items.len() as isize
    }

    /// An error at the position of the next item: `line L column C: <msg>`, or line and column -1 at the end.
    pub fn errorf(&self, msg: impl Display) -> LexError {
        self.peek_one().unwrap_or(&OUT_OF_RANGE).errorf(msg)
    }

    /// Advances by one; whether the iterator is now on an item.
    #[allow(clippy::should_implement_trait)] // Go's cursor, paired with prev and restore; not an Iterator.
    pub fn next(&mut self) -> bool {
        self.idx += 1;
        self.idx < self.len()
    }

    /// The current item, or an EOF item at line and column -1 when the iterator is not on one.
    pub fn item(&self) -> &'l Item<'a> {
        usize::try_from(self.idx)
            .ok()
            .and_then(|i| self.items.get(i))
            .unwrap_or(&OUT_OF_RANGE)
    }

    /// Moves back by one, unless the iterator is on the first item or before it; whether it moved.
    pub fn prev(&mut self) -> bool {
        if self.idx > 0 {
            self.idx -= 1;
            return true;
        }
        false
    }

    /// Moves the iterator to a position [`ItemIterator::save`] returned.
    ///
    /// # Panics
    ///
    /// When the position is before the start (-1) or past the items, as Go's assertion does.
    pub fn restore(&mut self, pos: isize) {
        assert!(pos <= self.len() && pos >= -1, "Assert failed");
        self.idx = pos;
    }

    /// The current position, for [`ItemIterator::restore`].
    pub fn save(&self) -> isize {
        self.idx
    }

    /// The next `num` items, without consuming them.
    ///
    /// # Errors
    ///
    /// [`LexErrorEnum::PeekOutOfRange`] when fewer than `num` items follow.
    pub fn peek(&self, num: usize) -> Result<&'l [Item<'a>], LexError> {
        let from = self.idx + 1;
        let to = from + num as isize;
        if to > self.len() {
            return Err(LexError::new(LexErrorEnum::PeekOutOfRange));
        }
        Ok(&self.items[from as usize..to as usize])
    }

    /// The next item, without consuming it.
    pub fn peek_one(&self) -> Option<&'l Item<'a>> {
        usize::try_from(self.idx + 1)
            .ok()
            .and_then(|i| self.items.get(i))
    }
}
