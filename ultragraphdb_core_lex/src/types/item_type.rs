/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/lexer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use core::fmt::{Display, Formatter};

/// The type of an emitted item. A client lexer declares its own as constants, from 5 up; 0 and 1 are
/// [`ITEM_EOF`] and [`ITEM_ERROR`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemType(i32);

/// Emitted when the end of the input is reached.
pub const ITEM_EOF: ItemType = ItemType::new(0);
/// Emitted when there was an error lexing the input.
pub const ITEM_ERROR: ItemType = ItemType::new(1);

impl ItemType {
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> i32 {
        self.0
    }
}

impl Display for ItemType {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
