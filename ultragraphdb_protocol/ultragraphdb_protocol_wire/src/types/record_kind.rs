/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a write-set record logs (`PROTOCOL.md` §12.2), its fixed part's `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordKind {
    /// 1: a transaction, or a Drop, whose record holds its drop as an operation.
    Transaction,
    /// 2: a registration the partition accepted as an entity's home.
    Registration,
    /// 3: an index build's start or completion.
    Build,
}

impl RecordKind {
    /// The kind's byte: 1, 2 or 3.
    pub fn code(self) -> u8 {
        match self {
            RecordKind::Transaction => 1,
            RecordKind::Registration => 2,
            RecordKind::Build => 3,
        }
    }

    /// The kind of `code`; `None` for a byte naming none.
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(RecordKind::Transaction),
            2 => Some(RecordKind::Registration),
            3 => Some(RecordKind::Build),
            _ => None,
        }
    }
}
