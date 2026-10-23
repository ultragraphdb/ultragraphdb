/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod item_display;

use std::borrow::Cow;
use std::fmt::Display;

use crate::errors::lex_error::{LexError, LexErrorEnum};
use crate::types::item_type::{ITEM_EOF, ItemType};

/// A unit emitted by the lexer: its type, its text and the line and column (in bytes) where it starts.
///
/// The text borrows the input; an error item owns its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item<'a> {
    typ: ItemType,
    val: Cow<'a, str>,
    line: i32,
    column: i32,
}

impl<'a> Item<'a> {
    pub(crate) fn new(typ: ItemType, val: Cow<'a, str>, line: i32, column: i32) -> Self {
        Self {
            typ,
            val,
            line,
            column,
        }
    }

    /// The item an iterator answers outside its items: type EOF, no text, line and column -1.
    pub(crate) const fn out_of_range() -> Item<'static> {
        Item {
            typ: ITEM_EOF,
            val: Cow::Borrowed(""),
            line: -1,
            column: -1,
        }
    }

    pub fn typ(&self) -> ItemType {
        self.typ
    }

    pub fn val(&self) -> &str {
        &self.val
    }

    pub fn line(&self) -> i32 {
        self.line
    }

    pub fn column(&self) -> i32 {
        self.column
    }

    /// An error at this item's position: `line L column C: <msg>`.
    pub fn errorf(&self, msg: impl Display) -> LexError {
        LexError::new(LexErrorEnum::Located {
            line: self.line,
            column: self.column,
            message: msg.to_string(),
        })
    }
}
