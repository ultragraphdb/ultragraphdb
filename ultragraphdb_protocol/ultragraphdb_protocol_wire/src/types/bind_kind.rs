/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a Bind binds to a partition (`PROTOCOL.md` §13.10), its command header's `kind` byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindKind {
    /// 1: a read-write transaction, whose Write, Operation, Commit and Abort frames follow on the stream.
    Transaction,
    /// 2: a Drop, whose reply ends the stream.
    Drop,
}

impl BindKind {
    /// The kind's byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            BindKind::Transaction => 1,
            BindKind::Drop => 2,
        }
    }

    /// The kind of byte `number`; `None` for any byte but 1 and 2.
    #[must_use]
    pub fn from_number(number: u8) -> Option<BindKind> {
        match number {
            1 => Some(BindKind::Transaction),
            2 => Some(BindKind::Drop),
            _ => None,
        }
    }
}
