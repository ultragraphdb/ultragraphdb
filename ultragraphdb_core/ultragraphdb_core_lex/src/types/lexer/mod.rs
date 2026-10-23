/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod lexer_accept;
mod lexer_emit;
mod lexer_quoted_string;
mod lexer_run;
mod lexer_scan;

use crate::types::item::Item;
use crate::types::rune_width::RuneWidth;
use crate::types::state_fn::StateFn;

/// Converts an input into items, driven by state functions (see [`Lexer::run`]).
///
/// `start..pos` is the text of the item being lexed; `line` and `column` (in bytes) are the position of `start`.
/// The depths and the mode are the client's state: Dgraph's DQL lexer nests braces in `depth`, round brackets in
/// `arg_depth` and blocks in `block_depth`, and keeps the state to return to after a token in `mode`.
#[derive(Debug, Clone)]
pub struct Lexer<'a> {
    input: &'a str,
    start: usize,
    pos: usize,
    width_stack: Vec<RuneWidth>,
    items: Vec<Item<'a>>,
    depth: i32,
    block_depth: i32,
    arg_depth: i32,
    mode: Option<StateFn>,
    line: i32,
    column: i32,
}

impl<'a> Lexer<'a> {
    /// A lexer at the start of the input, on line 1.
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            start: 0,
            pos: 0,
            width_stack: Vec::new(),
            items: Vec::new(),
            depth: 0,
            block_depth: 0,
            arg_depth: 0,
            mode: None,
            line: 1,
            column: 0,
        }
    }

    /// Starts over on a new input, keeping the allocated buffers.
    pub fn reset(&mut self, input: &'a str) {
        let mut items = std::mem::take(&mut self.items);
        let mut width_stack = std::mem::take(&mut self.width_stack);
        items.clear();
        width_stack.clear();
        *self = Self {
            items,
            width_stack,
            ..Self::new(input)
        };
    }

    /// The whole input. The slice borrows the input, not the lexer.
    pub fn input(&self) -> &'a str {
        self.input
    }

    /// Byte offset where the current item starts.
    pub fn start(&self) -> usize {
        self.start
    }

    /// Byte offset of the next rune to read.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Nesting of `{}`.
    pub fn depth(&self) -> i32 {
        self.depth
    }

    pub fn set_depth(&mut self, depth: i32) {
        self.depth = depth;
    }

    /// Nesting of blocks (e.g. a mutation block inside an upsert block).
    pub fn block_depth(&self) -> i32 {
        self.block_depth
    }

    pub fn set_block_depth(&mut self, block_depth: i32) {
        self.block_depth = block_depth;
    }

    /// Nesting of `()`.
    pub fn arg_depth(&self) -> i32 {
        self.arg_depth
    }

    pub fn set_arg_depth(&mut self, arg_depth: i32) {
        self.arg_depth = arg_depth;
    }

    /// The state to go back to after reading a token; `None` until a state sets it.
    pub fn mode(&self) -> Option<StateFn> {
        self.mode
    }

    pub fn set_mode(&mut self, mode: StateFn) {
        self.mode = Some(mode);
    }

    /// Line of `start`, from 1.
    pub fn line(&self) -> i32 {
        self.line
    }

    /// Column of `start` in bytes, from 0.
    pub fn column(&self) -> i32 {
        self.column
    }
}
