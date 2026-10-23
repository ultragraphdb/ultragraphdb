/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::borrow::Cow;
use std::fmt::Display;

use super::Lexer;
use crate::types::item::Item;
use crate::types::item_type::{ITEM_EOF, ITEM_ERROR, ItemType};
use crate::types::state_fn::StateFn;
use crate::utils::lexer::is_end_of_line;

impl<'a> Lexer<'a> {
    /// Emits an error item, `while lexing <input> at line L column C: <msg>`, and returns the state that stops the
    /// lexer.
    pub fn errorf(&mut self, msg: impl Display) -> Option<StateFn> {
        let val = format!(
            "while lexing {} at line {} column {}: {}",
            self.input, self.line, self.column, msg
        );
        self.items.push(Item::new(
            ITEM_ERROR,
            Cow::Owned(val),
            self.line,
            self.column,
        ));
        None
    }

    /// Emits `start..pos` as an item of type `t` and moves `start` to `pos`. Nothing is emitted when `pos` is
    /// before `start`, except EOF.
    pub fn emit(&mut self, t: ItemType) {
        if t != ITEM_EOF && self.pos < self.start {
            // Let ItemEOF go through.
            return;
        }
        let val = Cow::Borrowed(&self.input[self.start..self.pos]);
        self.items.push(Item::new(t, val, self.line, self.column));
        self.move_start_to_pos();
    }

    /// Skips `start..pos`: for runes without syntactical meaning (e.g. comments).
    pub fn ignore(&mut self) {
        self.move_start_to_pos();
    }

    fn move_start_to_pos(&mut self) {
        // check if we are about to move start to a new line
        for r in self.input[self.start..self.pos].chars() {
            if is_end_of_line(r) {
                self.line += 1;
                self.column = 0;
            } else {
                self.column += r.len_utf8() as i32;
            }
        }
        self.start = self.pos;
    }
}
