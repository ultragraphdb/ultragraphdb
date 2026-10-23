/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::Lexer;
use crate::types::rune_width::RuneWidth;

impl Lexer<'_> {
    fn push_width(&mut self, width: usize) {
        match self.width_stack.last_mut() {
            Some(last) if last.width == width => last.count += 1,
            _ => self.width_stack.push(RuneWidth { width, count: 1 }),
        }
    }

    /// Reads the next rune and advances past it; `None` at the end of the input.
    #[allow(clippy::should_implement_trait)] // Go's name; reading is undone by backup, not an Iterator.
    pub fn next(&mut self) -> Option<char> {
        match self.input[self.pos..].chars().next() {
            None => {
                self.push_width(0);
                None
            }
            Some(r) => {
                let width = r.len_utf8();
                self.push_width(width);
                self.pos += width;
                Some(r)
            }
        }
    }

    /// Moves back by the rune last read, including a read at the end of the input.
    ///
    /// # Panics
    ///
    /// When there is no read to undo, as Go's assertion does.
    pub fn backup(&mut self) {
        let rw = self
            .width_stack
            .last_mut()
            .expect("Backup should not be called when the width tracker stack is empty");
        let width = rw.width;
        if rw.count == 1 {
            self.width_stack.pop(); // pop the item from the stack
        } else {
            rw.count -= 1;
        }
        self.pos -= width;
    }

    /// The next rune, without advancing.
    pub fn peek(&mut self) -> Option<char> {
        let r = self.next();
        self.backup();
        r
    }

    /// The next two runes, without advancing.
    pub fn peek_two(&mut self) -> [Option<char>; 2] {
        let r1 = self.next();
        if r1.is_none() {
            self.backup();
            return [None, None];
        }
        let r2 = self.next();
        self.backup();
        self.backup();
        [r1, r2]
    }
}
